// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Tests for Section 5.3: get_requirement_status Tool

use crate::mcp::tests::{Test, TestContext};
use rmcp::model::{Annotated, CallToolRequestParam, Content, RawContent};
use serde_json::{Map, Value};
use std::sync::Arc;

fn make_args(req_identifier: &str) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert(
        "req_identifier".to_string(),
        Value::String(req_identifier.to_string()),
    );
    map
}

fn parse_content(content: &Annotated<RawContent>) -> Value {
    match &content.raw {
        RawContent::Text(text) => serde_json::from_str(&text.text).unwrap(),
        _ => panic!("Expected text content"),
    }
}

#[tokio::test]
async fn test_get_requirement_status() {
    //= docs/rfcs/0001-mcp-server.md#5.3
    //= type=test
    //# - MUST track implementation status
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // First get a requirement identifier using search
    let search_result = client
        .call_tool(CallToolRequestParam {
            name: "search_requirements".into(),
            arguments: Some({
                let mut map = Map::new();
                map.insert("query".to_string(), Value::String("error".to_string()));
                map
            }),
        })
        .await
        .unwrap();

    let search_content = parse_content(&search_result.content[0]);
    let req_id = search_content[0]["identifier"].as_str().unwrap();

    // Now test getting the status
    let result = client
        .call_tool(CallToolRequestParam {
            name: "get_requirement_status".into(),
            arguments: Some(make_args(req_id)),
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    assert!(content.get("status").is_some());
    assert!(content["status"].is_string());
}

#[tokio::test]
async fn test_invalid_requirement() {
    //= docs/rfcs/0001-mcp-server.md#5.3
    //= type=test
    //# - MUST track implementation status
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test with a non-existent requirement ID
    let result = client
        .call_tool(CallToolRequestParam {
            name: "get_requirement_status".into(),
            arguments: Some(make_args("nonexistent_id")),
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    assert!(content.get("error").is_some());
    assert!(content["error"].as_str().unwrap().contains("not found"));
}

#[tokio::test]
async fn test_status_values() {
    //= docs/rfcs/0001-mcp-server.md#5.3
    //= type=test
    //# - MUST support multiple status values
    //# - MUST update status in real-time
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // First get a requirement identifier using search
    let search_result = client
        .call_tool(CallToolRequestParam {
            name: "search_requirements".into(),
            arguments: Some({
                let mut map = Map::new();
                map.insert("query".to_string(), Value::String("error".to_string()));
                map
            }),
        })
        .await
        .unwrap();

    let search_content = parse_content(&search_result.content[0]);
    let req_id = search_content[0]["identifier"].as_str().unwrap();

    // Get the status
    let result = client
        .call_tool(CallToolRequestParam {
            name: "get_requirement_status".into(),
            arguments: Some(make_args(req_id)),
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    let status = content["status"].as_str().unwrap();

    // Verify status is one of the valid values
    assert!(
        status == "done"
            || status == "in progress"
            || status == "not started"
            || status == "needs tests"
    );
}
