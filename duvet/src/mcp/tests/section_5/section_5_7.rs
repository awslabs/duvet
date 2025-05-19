// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Tests for Section 5.7: resolve_spec_id Tool

use crate::mcp::tests::{Test, TestContext};
use rmcp::model::{Annotated, CallToolRequestParam, Content, RawContent};
use serde_json::{Map, Value};
use std::sync::Arc;

fn make_args(url: &str) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert("url".to_string(), Value::String(url.to_string()));
    map
}

fn parse_content(content: &Annotated<RawContent>) -> Value {
    match &content.raw {
        RawContent::Text(text) => serde_json::from_str(&text.text).unwrap(),
        _ => panic!("Expected text content"),
    }
}

#[tokio::test]
async fn test_resolve_spec_id() {
    //= docs/rfcs/0001-mcp-server.md#5.7
    //= type=test
    //# - MUST resolve URLs to specification IDs
    //# - MUST validate specification existence
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test resolving a known specification URL
    let result = client
        .call_tool(CallToolRequestParam {
            name: "resolve_spec_id".into(),
            arguments: Some(make_args("docs/rfcs/0001-mcp-server.md")),
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    assert!(content.get("spec_id").is_some());
    assert!(content["spec_id"].is_string());
}

#[tokio::test]
async fn test_invalid_url() {
    //= docs/rfcs/0001-mcp-server.md#5.7
    //= type=test
    //# - MUST validate specification existence
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test with a non-existent specification URL
    let result = client
        .call_tool(CallToolRequestParam {
            name: "resolve_spec_id".into(),
            arguments: Some(make_args("docs/rfcs/nonexistent.md")),
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    assert!(content.get("error").is_some());
    assert!(content["error"].as_str().unwrap().contains("not found"));
}

#[tokio::test]
async fn test_url_resolution() {
    //= docs/rfcs/0001-mcp-server.md#5.7
    //= type=test
    //# - MUST handle multiple URL formats
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test resolving the same URL multiple times
    let url = "docs/rfcs/0001-mcp-server.md";
    let mut spec_id = String::new();

    // First resolution
    let result = client
        .call_tool(CallToolRequestParam {
            name: "resolve_spec_id".into(),
            arguments: Some(make_args(url)),
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    spec_id = content["spec_id"].as_str().unwrap().to_string();

    // Subsequent resolutions should return the same ID
    for _ in 0..3 {
        let result = client
            .call_tool(CallToolRequestParam {
                name: "resolve_spec_id".into(),
                arguments: Some(make_args(url)),
            })
            .await
            .unwrap();

        let content = parse_content(&result.content[0]);
        assert_eq!(content["spec_id"].as_str().unwrap(), spec_id);
    }
}

#[tokio::test]
async fn test_url_variants() {
    //= docs/rfcs/0001-mcp-server.md#5.7
    //= type=test
    //# - MUST handle multiple URL formats
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test different URL formats that should resolve to the same specification
    let urls = vec![
        "docs/rfcs/0001-mcp-server.md",
        "./docs/rfcs/0001-mcp-server.md",
        "file://docs/rfcs/0001-mcp-server.md",
    ];

    let mut spec_id = String::new();

    // Get the spec_id from the first URL
    let result = client
        .call_tool(CallToolRequestParam {
            name: "resolve_spec_id".into(),
            arguments: Some(make_args(&urls[0])),
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    spec_id = content["spec_id"].as_str().unwrap().to_string();

    // All URL variants should resolve to the same spec_id
    for url in &urls[1..] {
        let result = client
            .call_tool(CallToolRequestParam {
                name: "resolve_spec_id".into(),
                arguments: Some(make_args(url)),
            })
            .await
            .unwrap();

        let content = parse_content(&result.content[0]);
        assert_eq!(content["spec_id"].as_str().unwrap(), spec_id);
    }
}
