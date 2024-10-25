// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{diagnostic::IntoDiagnostic, path::Path, Result};
use core::cell::RefCell;
use once_cell::sync::Lazy;
use std::sync::Arc;

static GLOBAL_ARGS: Lazy<Arc<[String]>> = Lazy::new(|| std::env::args().collect());
static GLOBAL_DIR: Lazy<Result<Path>> =
    Lazy::new(|| std::env::current_dir().map(|v| v.into()).into_diagnostic());

thread_local! {
    static ARGS: RefCell<Arc<[String]>> = RefCell::new(GLOBAL_ARGS.clone());
    static DIR: RefCell<Result<Path>> = RefCell::new(GLOBAL_DIR.clone());
}

pub fn args() -> Arc<[String]> {
    ARGS.with(|current| current.borrow().clone())
}

pub fn set_args(args: Arc<[String]>) {
    ARGS.with(|current| *current.borrow_mut() = args);
}

pub fn current_dir() -> Result<Path> {
    DIR.with(|current| current.borrow().clone())
}

pub fn set_current_dir(dir: Path) {
    DIR.with(|current| *current.borrow_mut() = Ok(dir));
}
