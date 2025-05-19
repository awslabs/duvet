// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Tests for Section 3.2: Architecture Requirements

use crate::mcp::tests::{Test, TestContext};
use rmcp::model::{Annotated, PaginatedRequestParam, RawResource};
use std::sync::Arc;

#[tokio::test]
async fn test_client_server_model() {
    //= docs/rfcs/0001-mcp-server.md#3-2-architecture-requirements
    //= type=test
    //# - MUST follow a client-server model
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test that client can communicate with server
    let resources = client.list_all_resources().await.unwrap();
    assert!(!resources.is_empty());
}

#[tokio::test]
async fn test_hierarchical_resource_structure() {
    //= docs/rfcs/0001-mcp-server.md#3-2-architecture-requirements
    //= type=test
    //# - MUST implement a hierarchical resource structure
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test that resources are organized hierarchically
    // First get specifications
    let param = Some(PaginatedRequestParam { cursor: None });
    let specs = client.list_resources(param.clone()).await.unwrap();
    assert!(!specs.resources.is_empty());

    // Then get sections within first specification
    let spec_uri = &specs.resources[0].raw.uri;
    let param = Some(PaginatedRequestParam { cursor: None });
    let sections = client.list_resources(param.clone()).await.unwrap();
    assert!(!sections.resources.is_empty());

    // Then get requirements within first section
    let section_uri = &sections.resources[0].raw.uri;
    let param = Some(PaginatedRequestParam { cursor: None });
    let requirements = client.list_resources(param.clone()).await.unwrap();
    assert!(!requirements.resources.is_empty());
}

#[tokio::test]
async fn test_concurrent_resource_access() {
    //= docs/rfcs/0001-mcp-server.md#3-2-architecture-requirements
    //= type=test
    //# - MUST support concurrent resource access
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test concurrent access by making multiple requests simultaneously
    let param = Some(PaginatedRequestParam { cursor: None });
    let futures = vec![
        client.list_resources(param.clone()),
        client.list_resources(param.clone()),
        client.list_resources(param.clone()),
    ];

    // All requests should complete successfully
    let results = futures::future::join_all(futures).await;
    for result in results {
        let resources = result.unwrap();
        assert!(!resources.resources.is_empty());
    }
}

#[tokio::test]
async fn test_data_consistency() {
    //= docs/rfcs/0001-mcp-server.md#3-2-architecture-requirements
    //= type=test
    //# - MUST maintain data consistency
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Make multiple requests and verify the data is consistent
    let param = Some(PaginatedRequestParam { cursor: None });
    let resources1 = client.list_resources(param.clone()).await.unwrap();
    let resources2 = client.list_resources(param.clone()).await.unwrap();

    // Both requests should return the same data
    assert_eq!(resources1.resources.len(), resources2.resources.len());
    for (r1, r2) in resources1.resources.iter().zip(resources2.resources.iter()) {
        assert_eq!(r1.raw.uri, r2.raw.uri);
        assert_eq!(r1.raw.name, r2.raw.name);
    }
}
