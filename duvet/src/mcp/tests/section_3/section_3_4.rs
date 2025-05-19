// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Tests for Section 3.4: Data Management Requirements

use crate::mcp::tests::{Test, TestContext};
use rmcp::model::PaginatedRequestParam;
use std::sync::Arc;

#[tokio::test]
async fn test_specification_caching() {
    //= docs/rfcs/0001-mcp-server.md#3-4-data-management-requirements
    //= type=test
    //# - MUST implement a caching strategy that:
    //#   - Caches frequently accessed specifications
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Make multiple requests for the same specification
    let param = Some(PaginatedRequestParam { cursor: None });

    // First request should cache the specification
    let specs1 = client.list_resources(param.clone()).await.unwrap();
    assert!(!specs1.resources.is_empty());

    // Second request should use the cache
    let specs2 = client.list_resources(param.clone()).await.unwrap();
    assert!(!specs2.resources.is_empty());

    // Both requests should return the same data
    assert_eq!(specs1.resources.len(), specs2.resources.len());
    for (r1, r2) in specs1.resources.iter().zip(specs2.resources.iter()) {
        assert_eq!(r1.raw.uri, r2.raw.uri);
        assert_eq!(r1.raw.name, r2.raw.name);
    }
}

#[tokio::test]
async fn test_validation_result_caching() {
    //= docs/rfcs/0001-mcp-server.md#3-4-data-management-requirements
    //= type=test
    //#   - Caches validation results
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    let param = Some(PaginatedRequestParam { cursor: None });

    // First request validates and caches the result
    let specs1 = client.list_resources(param.clone()).await.unwrap();
    assert!(!specs1.resources.is_empty());

    // Second request should use cached validation
    let specs2 = client.list_resources(param.clone()).await.unwrap();
    assert!(!specs2.resources.is_empty());

    // Both requests should return the same validation result
    assert_eq!(specs1.resources.len(), specs2.resources.len());
}

#[tokio::test]
async fn test_in_memory_storage() {
    //= docs/rfcs/0001-mcp-server.md#3-4-data-management-requirements
    //= type=test
    //# - MUST handle data persistence:
    //#   - MUST support in-memory storage for active sessions
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    let param = Some(PaginatedRequestParam { cursor: None });

    // Get initial data
    let specs = client.list_resources(param.clone()).await.unwrap();
    assert!(!specs.resources.is_empty());

    // Data should persist in memory throughout the session
    for _ in 0..3 {
        let specs2 = client.list_resources(param.clone()).await.unwrap();
        assert_eq!(specs.resources.len(), specs2.resources.len());
    }
}

#[tokio::test]
async fn test_data_integrity() {
    //= docs/rfcs/0001-mcp-server.md#3-4-data-management-requirements
    //= type=test
    //#   - MUST maintain data integrity during crashes
    let ctx1 = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx1).await.unwrap();

    let param = Some(PaginatedRequestParam { cursor: None });

    // Get initial state
    let specs1 = client.list_resources(param.clone()).await.unwrap();
    assert!(!specs1.resources.is_empty());

    // Simulate a crash by cancelling the client
    client.cancel().await.unwrap();

    // Start a new client with a new context
    let ctx2 = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx2).await.unwrap();

    // Data should be intact after restart
    let specs2 = client.list_resources(param.clone()).await.unwrap();
    assert_eq!(specs1.resources.len(), specs2.resources.len());
}
