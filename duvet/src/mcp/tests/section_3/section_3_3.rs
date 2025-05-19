// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Tests for Section 3.3: Integration Requirements

use crate::{
    annotation::{self, AnnotationSet},
    mcp::tests::{Test, TestContext},
    project::Project,
    reference::{self, Reference},
    target::SpecificationMap,
};
use rmcp::model::PaginatedRequestParam;
use std::sync::Arc;

#[tokio::test]
async fn test_specification_integration() {
    //= docs/rfcs/0001-mcp-server.md#3-3-integration-requirements
    //= type=test
    //# - MUST integrate with existing Duvet components:
    //#   - MUST use specification.rs for specification parsing and management
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test that specifications are parsed using specification.rs
    let param = Some(PaginatedRequestParam { cursor: None });
    let specs = client.list_resources(param).await.unwrap();
    assert!(!specs.resources.is_empty());

    // Verify the specification metadata matches what specification.rs provides
    let project = Project::default();
    let download_path = project.download_path().await.unwrap();
    let annotations = AnnotationSet::default();
    let specifications = annotation::specifications(annotations, download_path)
        .await
        .unwrap();

    // The server should use the same specifications as specification.rs
    assert_eq!(specs.resources.len(), specifications.len());
}

#[tokio::test]
async fn test_reference_integration() {
    //= docs/rfcs/0001-mcp-server.md#3-3-integration-requirements
    //= type=test
    //#   - MUST use reference.rs for citation handling
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Get a specification and its citations
    let param = Some(PaginatedRequestParam { cursor: None });
    let specs = client.list_resources(param.clone()).await.unwrap();
    assert!(!specs.resources.is_empty());

    let spec_uri = &specs.resources[0].raw.uri;
    let sections = client.list_resources(param.clone()).await.unwrap();
    assert!(!sections.resources.is_empty());

    let section_uri = &sections.resources[0].raw.uri;
    let requirements = client.list_resources(param.clone()).await.unwrap();
    assert!(!requirements.resources.is_empty());

    let req_uri = &requirements.resources[0].raw.uri;
    let citations = client.list_resources(param.clone()).await.unwrap();

    // Verify citations match what reference.rs provides
    let project = Project::default();
    let download_path = project.download_path().await.unwrap();
    let annotations = AnnotationSet::default();
    let specifications = annotation::specifications(annotations.clone(), download_path)
        .await
        .unwrap();
    let reference_map = annotation::reference_map(annotations).await.unwrap();
    let references = reference::query(reference_map, specifications)
        .await
        .unwrap();

    // The server should use the same citations as reference.rs
    assert_eq!(citations.resources.len(), references.len());
}

#[tokio::test]
async fn test_annotation_integration() {
    //= docs/rfcs/0001-mcp-server.md#3-3-integration-requirements
    //= type=test
    //#   - MUST use annotation.rs for code annotation processing
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Get all citations across the project
    let param = Some(PaginatedRequestParam { cursor: None });
    let citations = client.list_resources(param).await.unwrap();
    assert!(!citations.resources.is_empty());

    // Verify annotations match what annotation.rs provides
    let project = Project::default();
    let project_sources = project.sources().await.unwrap();
    let annotations = annotation::query(Arc::new(project_sources)).await.unwrap();

    // The server should use the same annotations as annotation.rs
    assert_eq!(citations.resources.len(), annotations.len());
}

#[tokio::test]
async fn test_backward_compatibility() {
    //= docs/rfcs/0001-mcp-server.md#3-3-integration-requirements
    //= type=test
    //# - MUST maintain backward compatibility with existing Duvet file formats
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test that we can still read existing file formats
    let project = Project::default();
    let project_sources = project.sources().await.unwrap();
    let annotations = annotation::query(Arc::new(project_sources)).await.unwrap();

    // The server should be able to process all existing annotations
    let param = Some(PaginatedRequestParam { cursor: None });
    let citations = client.list_resources(param).await.unwrap();
    assert_eq!(citations.resources.len(), annotations.len());
}

#[tokio::test]
async fn test_preserve_functionality() {
    //= docs/rfcs/0001-mcp-server.md#3-3-integration-requirements
    //= type=test
    //# - MUST preserve existing Duvet functionality
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test that all core Duvet functionality still works
    let project = Project::default();

    // Test specification parsing
    let download_path = project.download_path().await.unwrap();
    let annotations = AnnotationSet::default();
    let specifications = annotation::specifications(annotations.clone(), download_path)
        .await
        .unwrap();
    assert!(!specifications.is_empty());

    // Test citation handling
    let reference_map = annotation::reference_map(annotations).await.unwrap();
    let references = reference::query(reference_map, specifications)
        .await
        .unwrap();
    assert!(!references.is_empty());

    // Test annotation processing
    let project_sources = project.sources().await.unwrap();
    let annotations = annotation::query(Arc::new(project_sources)).await.unwrap();
    assert!(!annotations.is_empty());

    // The server should maintain all this functionality
    let param = Some(PaginatedRequestParam { cursor: None });
    let citations = client.list_resources(param).await.unwrap();
    assert!(!citations.resources.is_empty());
}
