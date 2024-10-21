// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::query::Query;
use core::marker::PhantomData;
use fxhash::FxHashMap;
use std::{
    any::{Any, TypeId},
    cell::RefCell,
    collections::hash_map::Entry,
    future::Future,
    hash::Hash,
    pin::Pin,
    sync::{Arc, RwLock},
};
use tracing::trace;

#[derive(Clone, Default)]
pub struct Cache(Arc<Set>);

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

impl Cache {
    pub fn current() -> Self {
        CACHE.with(|v| v.borrow().clone())
    }

    pub fn setup_thread(&self) {
        CACHE.with(|current| *current.borrow_mut() = self.clone());
    }

    pub fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        // swap the previous value with the new one for the closure invocation
        let prev = CACHE.with(|prev| core::mem::replace(&mut *prev.borrow_mut(), self.clone()));

        let result = f();

        // put the previous value back
        CACHE.with(|current| *current.borrow_mut() = prev);

        result
    }

    pub async fn with_async<F, R>(self, mut f: F) -> R
    where
        F: Future<Output = R> + Unpin,
    {
        futures::future::poll_fn(|cx| self.with(|| Pin::new(&mut f).poll(cx))).await
    }

    /// Caches a query with the given key
    pub fn get_or_init<K, V, R>(&self, key: K, resolver: R) -> Query<V>
    where
        K: 'static + Eq + Hash + Send + Sync,
        V: 'static + Send + Sync,
        R: 'static + FnOnce() -> Query<V>,
    {
        self.0.current.type_map(|map: &TypeMap<K, V, R>| {
            map.get_or_init(key, resolver, |key| self.0.previous.get::<_, _, R>(key))
        })
    }

    /// Caches a query for only the current cycle
    pub fn get_or_init_tmp<K, V, R>(&self, key: K, resolver: R) -> Query<V>
    where
        K: 'static + Eq + Hash + Send + Sync,
        V: 'static + Send + Sync,
        R: 'static + FnOnce() -> Query<V>,
    {
        self.0
            .temporary
            .type_map(|map: &TypeMap<K, V, R>| map.get_or_init(key, resolver, |_| None))
    }

    /// Caches a query globally
    pub fn get_or_init_global<K, V, R>(&self, key: K, resolver: R) -> Query<V>
    where
        K: 'static + Eq + Hash + Send + Sync,
        V: 'static + Send + Sync,
        R: 'static + FnOnce() -> Query<V>,
    {
        self.0
            .globals
            .type_map(|map: &TypeMap<K, V, R>| map.get_or_init(key, resolver, |_| None))
    }

    // pub fn get<K: 'static + Eq + Hash, V: 'static + Send + Sync>(
    //     &self,
    //     key: &K,
    // ) -> Option<Query<V>> {
    //     self.0.current.get(key)
    // }

    pub fn cycle(&self) {
        let mut previous = self.0.previous.map.write().unwrap();
        let mut current = self.0.current.map.write().unwrap();
        core::mem::swap(&mut *current, &mut *previous);
        current.clear();

        // temporary queries don't persist between cycles
        let mut temporary = self.0.temporary.map.write().unwrap();
        temporary.clear();
    }
}

#[derive(Default)]
struct Set {
    globals: Generation,
    current: Generation,
    previous: Generation,
    temporary: Generation,
}

#[derive(Default)]
struct Generation {
    map: RwLock<FxHashMap<TypeId, Box<dyn Any + Send + Sync>>>,
}

impl Generation {
    #[inline]
    fn type_map<T, M, R>(&self, map: M) -> R
    where
        T: 'static + Default + Send + Sync,
        M: FnOnce(&T) -> R,
    {
        loop {
            let l = self.map.read().unwrap();
            if let Some(v) = l.get(&TypeId::of::<T>()) {
                return map(v.downcast_ref::<T>().unwrap());
            }
            drop(l);

            let mut l = self.map.write().unwrap();
            if let Entry::Vacant(entry) = l.entry(TypeId::of::<T>()) {
                let map = T::default();
                entry.insert(Box::new(map));
            }
            drop(l);
        }
    }

    fn get<K, V, R>(&self, key: &K) -> Option<Query<V>>
    where
        K: 'static + Eq + Hash,
        V: 'static + Send + Sync,
        R: 'static + FnOnce() -> Query<V>,
    {
        let l = self.map.read().unwrap();
        let v = l.get(&TypeId::of::<(K, V, R)>())?;
        v.downcast_ref::<TypeMap<K, V, R>>().unwrap().get(key)
    }
}

pub(crate) struct TypeMap<K, V, R> {
    map: RwLock<FxHashMap<K, Query<V>>>,
    _resolver: PhantomData<R>,
}

// SAFETY: we don't store the actual future on the map, just the PhantomData
unsafe impl<K: Send, V: Send, R> Send for TypeMap<K, V, R> {}
unsafe impl<K: Sync, V: Sync, R> Sync for TypeMap<K, V, R> {}

impl<K, V, R> Default for TypeMap<K, V, R> {
    fn default() -> Self {
        Self {
            map: RwLock::new(FxHashMap::default()),
            _resolver: PhantomData,
        }
    }
}

impl<K, V, R> TypeMap<K, V, R>
where
    K: 'static + Eq + Hash,
    V: 'static + Send + Sync,
    R: FnOnce() -> Query<V>,
{
    fn get_or_init<P>(&self, key: K, resolver: R, prev: P) -> Query<V>
    where
        P: FnOnce(&K) -> Option<Query<V>>,
    {
        if let Some(value) = self.get(&key) {
            trace!(
                key = %core::any::type_name::<K>(),
                value = %core::any::type_name::<V>(),
                status = %"cached"
            );
            return value;
        }

        if let Some(v) = prev(&key) {
            let mut l = self.map.write().unwrap();

            return match l.entry(key) {
                Entry::Occupied(entry) => {
                    trace!(
                        key = %core::any::type_name::<K>(),
                        value = %core::any::type_name::<V>(),
                        status = %"cached"
                    );
                    entry.get().clone()
                }
                Entry::Vacant(entry) => {
                    trace!(
                        key = %core::any::type_name::<K>(),
                        value = %core::any::type_name::<V>(),
                        status = %"recached"
                    );
                    entry.insert(v).clone()
                }
            };
        }

        let mut l = self.map.write().unwrap();
        match l.entry(key) {
            Entry::Occupied(entry) => {
                trace!(
                    key = %core::any::type_name::<K>(),
                    value = %core::any::type_name::<V>(),
                    status = %"cached"
                );
                entry.get().clone()
            }
            Entry::Vacant(entry) => {
                let q = resolver();
                trace!(
                    key = %core::any::type_name::<K>(),
                    value = %core::any::type_name::<V>(),
                    status = %"computed"
                );
                entry.insert(q).clone()
            }
        }
    }

    fn get(&self, key: &K) -> Option<Query<V>> {
        let l = self.map.read().unwrap();
        l.get(key).cloned()
    }
}
