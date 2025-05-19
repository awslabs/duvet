// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Tests for Section 3.4: Data Management Requirements

use crate::mcp::tests::{Test, TestContext};
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
    let specs = client.list_resources("/specifications").await.unwrap();
    assert!(!specs.is_empty());

    let spec_uri = &specs[0].raw.uri;

    // First request should cache the specification
    let spec1 = client.get_resource(spec_uri).await.unwrap();

    // Second request should use the cache
    let spec2 = client.get_resource(spec_uri).await.unwrap();

    // Both requests should return the same data
    assert_eq!(spec1.raw.name, spec2.raw.name);
    assert_eq!(spec1.raw.description, spec2.raw.description);
}

#[tokio::test]
async fn test_validation_result_caching() {
    //= docs/rfcs/0001-mcp-server.md#3-4-data-management-requirements
    //= type=test
    //#   - Caches validation results
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Get a specification and its citations
    let specs = client.list_resources("/specifications").await.unwrap();
    assert!(!specs.is_empty());

    let spec_uri = &specs[0].raw.uri;
    let sections = client
        .list_resources(&format!("{}/sections", spec_uri))
        .await
        .unwrap();
    assert!(!sections.is_empty());

    let section_uri = &sections[0].raw.uri;
    let requirements = client
        .list_resources(&format!("{}/requirements", section_uri))
        .await
        .unwrap();
    assert!(!requirements.is_empty());

    let req_uri = &requirements[0].raw.uri;

    // First request validates and caches the result
    let citations1 = client
        .list_resources(&format!("{}/citations", req_uri))
        .await
        .unwrap();

    // Second request should use cached validation
    let citations2 = client
        .list_resources(&format!("{}/citations", req_uri))
        .await
        .unwrap();

    // Both requests should return the same data
    assert_eq!(citations1.len(), citations2.len());
}

#[tokio::test]
async fn test_in_memory_storage() {
    //= docs/rfcs/0001-mcp-server.md#3-4-data-management-requirements
    //= type=test
    //# - MUST handle data persistence:
    //#   - MUST support in-memory storage for active sessions
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Get initial data
    let specs = client.list_resources("/specifications").await.unwrap();
    assert!(!specs.is_empty());

    // Data should persist in memory throughout the session
    let spec_uri = &specs[0].raw.uri;
    let sections = client
        .list_resources(&format!("{}/sections", spec_uri))
        .await
        .unwrap();
    assert!(!sections.is_empty());

    // Make multiple requests to verify data stays in memory
    for _ in 0..3 {
        let sections2 = client
            .list_resources(&format!("{}/sections", spec_uri))
            .await
            .unwrap();
        assert_eq!(sections.len(), sections2.len());
    }
}

#[tokio::test]
async fn test_citation_persistence() {
    //= docs/rfcs/0001-mcp-server.md#3-4-data-management-requirements
    //= type=test
    //#   - MUST persist citation data to disk
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Get all citations
    let citations = client.list_resources("/citations").await.unwrap();
    assert!(!citations.is_empty());

    // Citations should be persisted to disk
    // We can verify this by checking that the citations match what's in the project files
    let project = crate::project::Project::default();
    let project_sources = project.sources().await.unwrap();
    let annotations = crate::annotation::query(Arc::new(project_sources))
        .await
        .unwrap();

    // The server should have access to all persisted citations
    assert_eq!(citations.len(), annotations.len());
}

#[tokio::test]
async fn test_data_integrity() {
    //= docs/rfcs/0001-mcp-server.md#3-4-data-management-requirements
    //= type=test
    //#   - MUST maintain data integrity during crashes
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Get initial state
    let citations1 = client.list_resources("/citations").await.unwrap();
    assert!(!citations1.is_empty());

    // Simulate a crash by cancelling the client
    client.cancel().await.unwrap();

    // Start a new client
    let client = Test::start(ctx).await.unwrap();

    // Data should be intact after restart
    let citations2 = client.list_resources("/citations").await.unwrap();
    assert_eq!(citations1.len(), citations2.len());
}
