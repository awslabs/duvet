// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use duvet_core::{query, Query};

#[query]
async fn add(a: Query<u64>, b: Query<u64>) -> u64 {
    let a = *a.get().await;
    let b = *b.get().await;
    a + b
}

#[query(cache)]
async fn args() -> Vec<String> {
    std::env::args().collect()
}

#[query(delegate)]
async fn delegate() -> Vec<String> {
    args()
}

#[query(cache)]
async fn add_cache(a: Query<u64>, b: Query<u64>) -> u64 {
    let a = *a.get().await;
    let b = *b.get().await;
    a + b
}

#[query(cache)]
async fn mixed_cache(a: Query<u64>, b: u64) -> u64 {
    let a = *a.get().await;
    a + b
}

#[query(cache)]
async fn ignored_cache(a: Query<u64>, b: Query<u64>, #[skip] log: bool) -> u64 {
    let a = *a.get().await;
    let b = *b.get().await;
    let value = a + b;

    if log {
        dbg!(value);
    }

    value
}

#[query(cache, delegate)]
async fn cache_delegate(a: Query<bool>) -> Vec<String> {
    if a.await {
        args()
    } else {
        delegate()
    }
}
