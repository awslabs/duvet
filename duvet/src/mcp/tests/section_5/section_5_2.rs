// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Tests for Section 5.2: search_requirements Tool

use crate::mcp::tests::{Test, TestContext};
use rmcp::model::{Annotated, CallToolRequestParam, Content, RawContent};
use serde_json::{Map, Value};
use std::sync::Arc;

fn make_args(query: &str) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert("query".to_string(), Value::String(query.to_string()));
    map
}

fn parse_content(content: &Annotated<RawContent>) -> Value {
    match &content.raw {
        RawContent::Text(text) => serde_json::from_str(&text.text).unwrap(),
        _ => panic!("Expected text content"),
    }
}

#[tokio::test]
async fn test_search_requirements() {
    //= docs/rfcs/0001-mcp-server.md#5.2
    //= type=test
    //# - MUST support keyword search
    //# - MUST search across all specifications
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test searching for a requirement
    let result = client
        .call_tool(CallToolRequestParam {
            name: "search_requirements".into(),
            arguments: Some(make_args("error handling")),
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    assert!(content.as_array().unwrap().len() > 0);

    // Verify each result has the required fields
    for requirement in content.as_array().unwrap() {
        assert!(requirement.get("identifier").is_some());
        assert!(requirement.get("full_path").is_some());
        assert!(requirement.get("text").is_some());
    }
}

#[tokio::test]
async fn test_search_no_results() {
    //= docs/rfcs/0001-mcp-server.md#5.2
    //= type=test
    //# - MUST support keyword search
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test searching with a query that should return no results
    let result = client
        .call_tool(CallToolRequestParam {
            name: "search_requirements".into(),
            arguments: Some(make_args("nonexistent requirement xyz123")),
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    assert_eq!(content.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_search_response_format() {
    //= docs/rfcs/0001-mcp-server.md#5.2
    //= type=test
    //# - MUST return matching requirements with context
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test searching for a requirement
    let result = client
        .call_tool(CallToolRequestParam {
            name: "search_requirements".into(),
            arguments: Some(make_args("error")),
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    assert!(content.is_array());

    // Check first result has all required fields
    if let Some(first_result) = content.as_array().unwrap().first() {
        assert!(first_result.get("identifier").unwrap().is_string());
        assert!(first_result.get("full_path").unwrap().is_string());
        assert!(first_result.get("text").unwrap().is_string());
    }
}
