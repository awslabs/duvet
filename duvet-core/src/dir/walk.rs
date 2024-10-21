// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::{DirIter, Path};
use crate::{glob::Glob, vfs};
use core::future::Future;
use futures::{stream, Stream};
use std::{future::ready, marker::PhantomData};

pub fn dir(root: Path) -> impl Stream<Item = Path> {
    filtered(root, |depth, _path| ready((depth <= 100).into()))
}

pub fn glob(root: Path, include: Glob, ignore: Glob) -> impl Stream<Item = Path> {
    filtered(root, move |depth, path| {
        let path = path.clone();
        let include = include.clone();
        let ignore = ignore.clone();
        async move {
            let mut is_ok = !ignore.is_match(&path);

            is_ok &= depth <= 100;

            if is_ok {
                match vfs::read_metadata(path.clone()).await.ok() {
                    Some(meta) if meta.is_dir() => return ControlFlow::Skip,
                    _ => {}
                }
            }

            is_ok &= include.is_match(&path);

            is_ok.into()
        }
    })
}

pub fn filtered<F, Fut>(root: Path, filter: F) -> impl Stream<Item = Path>
where
    F: FnMut(usize, &Path) -> Fut + Send,
    Fut: Future<Output = ControlFlow> + Send,
{
    let stream = State {
        start: Some(root),
        stack_list: vec![],
        depth: 0,
        filter,
        _f: PhantomData,
    };

    stream.walk_dir()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlFlow {
    Yield,
    Skip,
    Break,
}

impl From<bool> for ControlFlow {
    fn from(value: bool) -> Self {
        if value {
            Self::Yield
        } else {
            Self::Break
        }
    }
}

struct State<F, Fut> {
    /// The start path.
    ///
    /// This is only `Some(...)` at the beginning. After the first iteration,
    /// this is always `None`.
    start: Option<Path>,
    stack_list: Vec<DirIter>,
    /// The current depth of iteration (the length of the stack at the
    /// beginning of each iteration).
    depth: usize,

    filter: F,
    _f: PhantomData<Fut>,
}

impl<F, Fut> State<F, Fut>
where
    F: FnMut(usize, &Path) -> Fut + Send,
    Fut: Future<Output = ControlFlow> + Send,
{
    fn walk_dir(self) -> impl Stream<Item = Path> {
        stream::unfold(self, move |mut state| async move {
            if let Some(path) = state.start.take() {
                if let Some(entry) = state.handle_entry(path).await {
                    return Some((entry, state));
                }
            }

            while !state.stack_list.is_empty() {
                state.depth = state.stack_list.len();

                let next = state
                    .stack_list
                    .last_mut()
                    .expect("BUG: stack should be non-empty")
                    .next();

                match next {
                    None => state.pop(),
                    Some(path) => {
                        if let Some(entry) = state.handle_entry(path).await {
                            return Some((entry, state));
                        }
                    }
                }
            }

            None
        })
    }

    async fn handle_entry(&mut self, path: Path) -> Option<Path> {
        let result = (self.filter)(self.depth, &path).await;

        let should_yield = match result {
            ControlFlow::Break => return None,
            ControlFlow::Skip => false,
            ControlFlow::Yield => true,
        };

        let meta = vfs::read_metadata(path.clone()).await.ok()?;

        if meta.is_dir() {
            if let Ok(dir) = vfs::read_dir(path.clone()).await {
                self.stack_list.push(dir.into_iter());
            }
        }

        if should_yield {
            Some(path)
        } else {
            None
        }
    }

    fn pop(&mut self) {
        self.stack_list
            .pop()
            .expect("BUG: cannot pop from empty stack");
    }
}
