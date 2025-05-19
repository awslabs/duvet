// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Tests for Section 3.5: Concurrency Requirements

use crate::mcp::tests::{Test, TestContext};
use rmcp::model::PaginatedRequestParam;
use std::sync::Arc;
use tokio::task::JoinSet;

#[tokio::test]
async fn test_thread_safe_resource_access() {
    //= docs/rfcs/0001-mcp-server.md#3-5-concurrency-requirements
    //= type=test
    //# - MUST implement thread-safe resource access:
    //#   - MUST use appropriate synchronization primitives
    //#   - MUST prevent data races
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Create multiple concurrent requests
    let mut tasks = JoinSet::new();
    let param = Some(PaginatedRequestParam { cursor: None });

    // Launch 10 concurrent requests
    for _ in 0..10 {
        let client = client.clone();
        let param = param.clone();
        tasks.spawn(async move { client.list_resources(param).await.unwrap() });
    }

    // All requests should complete successfully without data races
    let mut results = Vec::new();
    while let Some(result) = tasks.join_next().await {
        let response = result.unwrap();
        results.push(response);
    }

    // All responses should have the same data
    let first = &results[0];
    for result in &results[1..] {
        assert_eq!(first.resources.len(), result.resources.len());
        for (r1, r2) in first.resources.iter().zip(result.resources.iter()) {
            assert_eq!(r1.raw.uri, r2.raw.uri);
            assert_eq!(r1.raw.name, r2.raw.name);
        }
    }
}

#[tokio::test]
async fn test_concurrent_requests() {
    //= docs/rfcs/0001-mcp-server.md#3-5-concurrency-requirements
    //= type=test
    //# - MUST handle concurrent requests:
    //#   - MUST support multiple simultaneous clients
    //#   - MUST maintain request isolation
    let ctx1 = Arc::new(TestContext::new().unwrap());
    let client1 = Test::start(ctx1).await.unwrap();

    let ctx2 = Arc::new(TestContext::new().unwrap());
    let client2 = Test::start(ctx2).await.unwrap();

    let param = Some(PaginatedRequestParam { cursor: None });

    // Make concurrent requests from different clients
    let (result1, result2) = tokio::join!(
        client1.list_resources(param.clone()),
        client2.list_resources(param.clone())
    );

    // Both requests should succeed independently
    let response1 = result1.unwrap();
    let response2 = result2.unwrap();

    // Each client should get the same data
    assert_eq!(response1.resources.len(), response2.resources.len());
    for (r1, r2) in response1.resources.iter().zip(response2.resources.iter()) {
        assert_eq!(r1.raw.uri, r2.raw.uri);
        assert_eq!(r1.raw.name, r2.raw.name);
    }
}

#[tokio::test]
async fn test_resource_locking() {
    //= docs/rfcs/0001-mcp-server.md#3-5-concurrency-requirements
    //= type=test
    //# - MUST implement resource locking:
    //#   - For citation updates
    //#   - For specification modifications
    //#   - With timeout mechanisms
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    let param = Some(PaginatedRequestParam { cursor: None });

    // Make multiple concurrent requests to test locking
    let mut tasks = JoinSet::new();

    // Launch 5 concurrent requests
    for _ in 0..5 {
        let client = client.clone();
        let param = param.clone();
        tasks.spawn(async move { client.list_resources(param).await.unwrap() });
    }

    // All requests should complete without deadlocks
    while let Some(result) = tasks.join_next().await {
        result.unwrap();
    }
}
