// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use futures::{future::poll_fn, ready, FutureExt};
use std::{
    cell::UnsafeCell,
    fmt,
    future::{Future, Pending},
    mem::MaybeUninit,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll},
};
use tokio::sync::Semaphore;

pub struct Query<T> {
    inner: Arc<dyn InnerState<Output = T>>,
}

impl<T> Clone for Query<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: 'static + Send + Sync + fmt::Debug> fmt::Debug for Query<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Query").field(&self.try_get()).finish()
    }
}

unsafe impl<T: Sync + Send> Sync for Query<T> {}
unsafe impl<T: Sync + Send> Send for Query<T> {}

impl<T: 'static + Send + Sync> From<T> for Query<T> {
    fn from(value: T) -> Self {
        let semaphore = Semaphore::new(0);
        semaphore.close();

        let future = UnsafeCell::new(FutureState::<Pending<T>>::Finished);

        let inner = Inner {
            value_set: AtomicBool::new(true),
            value: UnsafeCell::new(MaybeUninit::new(value)),
            semaphore,
            future,
        };

        Query {
            inner: Arc::new(inner),
        }
    }
}

impl<T: 'static + Send + Sync> Query<T> {
    pub fn new<F: 'static + Future<Output = T> + Send>(future: F) -> Self {
        let inner = Inner {
            value_set: AtomicBool::new(false),
            value: UnsafeCell::new(MaybeUninit::uninit()),
            semaphore: Semaphore::new(1),
            future: UnsafeCell::new(FutureState::Init(future)),
        };

        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn delegate<F: 'static + Future<Output = Query<T>> + Send>(future: F) -> Self {
        let inner = Inner {
            value_set: AtomicBool::new(false),
            value: UnsafeCell::new(MaybeUninit::uninit()),
            semaphore: Semaphore::new(1),
            future: UnsafeCell::new(FutureState::Init(future)),
        };

        let inner = Delegate {
            inner,
            query_fut: UnsafeCell::new(None),
        };

        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn spawn<F: 'static + Future<Output = T> + Send>(future: F) -> Self {
        let inner = Spawn {
            value_set: AtomicBool::new(false),
            value: UnsafeCell::new(MaybeUninit::uninit()),
            semaphore: Semaphore::new(1),
            future: UnsafeCell::new(SpawnFutureState::Init(future)),
        };

        Self {
            inner: Arc::new(inner),
        }
    }

    pub async fn get(&self) -> &T {
        if let Some(value) = self.try_get() {
            return value;
        }

        // Here we try to acquire the semaphore permit. Holding the permit
        // will allow us to set the value of the Query, and prevents
        // other tasks from initializing the Query while we are holding
        // it.
        if let Ok(permit) = self.inner.semaphore().acquire().await {
            debug_assert!(!self.inner.initialized());

            // If `f()` panics or `select!` is called, this
            // `get_or_init` call is aborted and the semaphore permit is
            // dropped.
            poll_fn(move |cx| unsafe {
                // SAFETY: polling is guarded by semaphores
                self.inner.poll(cx)
            })
            .await;

            permit.forget();
        }

        // SAFETY: The semaphore has been closed. This only happens
        // when the Query is fully initialized.
        unsafe { self.inner.get_unchecked() }
    }

    pub fn try_get(&self) -> Option<&T> {
        self.inner.try_get()
    }

    pub fn map<M, F, R>(&self, m: M) -> Query<R>
    where
        M: 'static + Send + FnOnce(&T) -> F,
        F: 'static + Send + Future<Output = R>,
        R: 'static + Send + Sync,
    {
        let inner = self.clone();
        Query::new(async move {
            let v = inner.get().await;
            m(v).await
        })
    }
}

impl<T: 'static + Clone + Send + Sync> Query<T> {
    pub async fn get_cloned(self) -> T {
        let value = self.get().await;
        value.clone()
    }

    pub fn map_cloned<M, F, R>(&self, m: M) -> Query<R>
    where
        M: 'static + Send + FnOnce(T) -> F,
        F: 'static + Send + Future<Output = R>,
        R: 'static + Send + Sync,
    {
        let inner = self.clone();
        Query::new(async move {
            let v = inner.get_cloned().await;
            m(v).await
        })
    }
}

impl<T> core::future::IntoFuture for Query<T>
where
    T: 'static + Clone + Send + Sync,
{
    type Output = T;
    type IntoFuture = Pin<Box<dyn 'static + Send + Future<Output = T>>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.get_cloned())
    }
}

trait InnerState: Send + Sync {
    type Output;

    fn try_get(&self) -> Option<&Self::Output>;
    unsafe fn get_unchecked(&self) -> &Self::Output;
    fn initialized(&self) -> bool;
    fn semaphore(&self) -> &Semaphore;
    unsafe fn poll(&self, cx: &mut Context) -> Poll<()>;
}

struct Inner<T, F> {
    value_set: AtomicBool,
    value: UnsafeCell<MaybeUninit<T>>,
    semaphore: Semaphore,
    future: UnsafeCell<FutureState<F>>,
}

unsafe impl<T: Sync + Send, F: Send> Sync for Inner<T, F> {}
unsafe impl<T: Sync + Send, F: Send> Send for Inner<T, F> {}

impl<T, F> Inner<T, F> {
    fn initialized(&self) -> bool {
        // Using acquire ordering so any threads that read a true from this
        // atomic is able to read the value.
        self.value_set.load(Ordering::Acquire)
    }
}

impl<T, F> Drop for Inner<T, F> {
    fn drop(&mut self) {
        if self.initialized() {
            unsafe {
                (*self.value.get()).assume_init_drop();
            }
        }
    }
}

impl<T, F> InnerState for Inner<T, F>
where
    T: Send + Sync,
    F: Send + Future<Output = T>,
{
    type Output = T;

    unsafe fn get_unchecked(&self) -> &Self::Output {
        debug_assert!(self.initialized());

        (*self.value.get()).assume_init_ref()
    }

    fn try_get(&self) -> Option<&T> {
        if self.initialized() {
            // SAFETY: The Query has been fully initialized.
            Some(unsafe { self.get_unchecked() })
        } else {
            None
        }
    }

    fn initialized(&self) -> bool {
        Inner::initialized(self)
    }

    fn semaphore(&self) -> &Semaphore {
        &self.semaphore
    }

    unsafe fn poll(&self, cx: &mut Context) -> Poll<()> {
        let future = &mut *self.future.get();
        match future {
            FutureState::Init(future) => {
                // the Inner is allocated and is stable
                let future = Pin::new_unchecked(future);
                let value = ready!(future.poll(cx));

                self.value.get().write(MaybeUninit::new(value));
                self.future.get().write(FutureState::Finished);

                // Using release ordering so any threads that read a true from this
                // atomic is able to read the value we just stored.
                self.value_set.store(true, Ordering::Release);
                self.semaphore.close();

                Poll::Ready(())
            }
            FutureState::Finished => {
                debug_assert!(self.initialized());
                Poll::Ready(())
            }
        }
    }
}

struct Delegate<T, F> {
    inner: Inner<Query<T>, F>,
    query_fut: UnsafeCell<Option<Pin<Box<dyn Future<Output = ()>>>>>,
}

unsafe impl<T: Sync + Send, F: Send> Sync for Delegate<T, F> {}
unsafe impl<T: Sync + Send, F: Send> Send for Delegate<T, F> {}

impl<T, F> InnerState for Delegate<T, F>
where
    T: 'static + Send + Sync,
    F: Send + Future<Output = Query<T>>,
{
    type Output = T;

    unsafe fn get_unchecked(&self) -> &Self::Output {
        self.inner.get_unchecked().inner.get_unchecked()
    }

    fn try_get(&self) -> Option<&T> {
        if self.initialized() {
            // SAFETY: The Query has been fully initialized.
            let query = unsafe { self.inner.get_unchecked() };
            if query.inner.initialized() {
                Some(unsafe { query.inner.get_unchecked() })
            } else {
                None
            }
        } else {
            None
        }
    }

    fn initialized(&self) -> bool {
        self.inner.initialized()
    }

    fn semaphore(&self) -> &Semaphore {
        &self.inner.semaphore
    }

    unsafe fn poll(&self, cx: &mut Context) -> Poll<()> {
        loop {
            if let Some(ref mut f) = &mut *self.query_fut.get() {
                ready!(f.poll_unpin(cx));

                *self.query_fut.get() = None;
                return Poll::Ready(());
            }

            ready!(self.inner.poll(cx));

            let query = self.inner.get_unchecked().clone();
            *self.query_fut.get() = Some(Box::pin(async move {
                // evaluate the nested query
                query.get().await;
            }));
        }
    }
}

struct Spawn<T, F> {
    value_set: AtomicBool,
    value: UnsafeCell<MaybeUninit<T>>,
    semaphore: Semaphore,
    future: UnsafeCell<SpawnFutureState<T, F>>,
}

unsafe impl<T: Sync + Send, F: Send> Sync for Spawn<T, F> {}
unsafe impl<T: Sync + Send, F: Send> Send for Spawn<T, F> {}

impl<T, F> InnerState for Spawn<T, F>
where
    T: 'static + Send + Sync,
    F: 'static + Send + Future<Output = T>,
{
    type Output = T;

    unsafe fn get_unchecked(&self) -> &Self::Output {
        debug_assert!(self.initialized());

        (*self.value.get()).assume_init_ref()
    }

    fn try_get(&self) -> Option<&T> {
        if self.initialized() {
            // SAFETY: The Query has been fully initialized.
            Some(unsafe { self.get_unchecked() })
        } else {
            None
        }
    }

    fn initialized(&self) -> bool {
        // Using acquire ordering so any threads that read a true from this
        // atomic is able to read the value.
        self.value_set.load(Ordering::Acquire)
    }

    fn semaphore(&self) -> &Semaphore {
        &self.semaphore
    }

    unsafe fn poll(&self, cx: &mut Context) -> Poll<()> {
        let future = &mut *self.future.get();
        loop {
            match core::mem::replace(future, SpawnFutureState::Finished) {
                SpawnFutureState::Init(fut) => {
                    let handle = tokio::spawn(fut);
                    *future = SpawnFutureState::Spawned(handle);
                }
                SpawnFutureState::Spawned(mut handle) => {
                    let value = match Pin::new(&mut handle).poll(cx) {
                        Poll::Ready(value) => value,
                        Poll::Pending => {
                            *future = SpawnFutureState::Spawned(handle);
                            return Poll::Pending;
                        }
                    };

                    return match value {
                        Ok(value) => {
                            self.value.get().write(MaybeUninit::new(value));
                            self.future.get().write(SpawnFutureState::Spawned(handle));

                            // Using release ordering so any threads that read a true from this
                            // atomic is able to read the value we just stored.
                            self.value_set.store(true, Ordering::Release);
                            self.semaphore.close();

                            Poll::Ready(())
                        }
                        Err(err) => match err.try_into_panic() {
                            Ok(reason) => std::panic::resume_unwind(reason),
                            Err(err) => panic!("{}", err),
                        },
                    };
                }
                SpawnFutureState::Finished => {
                    debug_assert!(self.initialized());
                    return Poll::Ready(());
                }
            }
        }
    }
}

enum FutureState<F> {
    Init(F),
    Finished,
}

enum SpawnFutureState<T, F> {
    Init(F),
    Spawned(tokio::task::JoinHandle<T>),
    Finished,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn query_test() {
        let (tx, rx) = oneshot::channel::<u64>();

        let query = Query::new(async move { rx.await.unwrap() });

        let a = query.clone();
        let a = tokio::spawn(async move { *a.get().await });

        let b = query;
        let b = tokio::spawn(async move { *b.get().await });

        tx.send(123).unwrap();

        assert_eq!(a.await.unwrap(), 123);
        assert_eq!(b.await.unwrap(), 123);
    }
}
