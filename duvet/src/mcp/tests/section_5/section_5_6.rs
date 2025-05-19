// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Tests for Section 5.6: get_citation_context Tool

use crate::mcp::tests::{Test, TestContext};
use rmcp::model::{Annotated, CallToolRequestParam, Content, RawContent};
use serde_json::{Map, Value};
use std::sync::Arc;

fn make_args(citation_id: &str, context_lines: i32) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert(
        "citation_id".to_string(),
        Value::String(citation_id.to_string()),
    );
    map.insert(
        "context_lines".to_string(),
        Value::Number(context_lines.into()),
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
async fn test_get_citation_context() {
    //= docs/rfcs/0001-mcp-server.md#5-6-citation-context-tool
    //= type=test
    //# - MUST retrieve surrounding code
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // First get a citation ID using search
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
    let citation_id = search_content[0]["citations"][0]["id"].as_str().unwrap();

    // Test getting context with 2 lines before and after
    let result = client
        .call_tool(CallToolRequestParam {
            name: "get_citation_context".into(),
            arguments: Some(make_args(citation_id, 2)),
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    assert!(content.get("file_path").is_some());
    assert!(content.get("line_number").is_some());
    assert!(content.get("context").is_some());

    // Verify context array has correct number of lines
    let context = content["context"].as_array().unwrap();
    assert_eq!(context.len(), 5); // 2 before + citation line + 2 after
}

#[tokio::test]
async fn test_invalid_citation_id() {
    //= docs/rfcs/0001-mcp-server.md#5-6-citation-context-tool
    //= type=test
    //# - MUST preserve code formatting
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test with a non-existent citation ID
    let result = client
        .call_tool(CallToolRequestParam {
            name: "get_citation_context".into(),
            arguments: Some(make_args("nonexistent.rs:42", 2)),
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    assert!(content.get("error").is_some());
    assert!(content["error"].as_str().unwrap().contains("not found"));
}

#[tokio::test]
async fn test_context_lines_parameter() {
    //= docs/rfcs/0001-mcp-server.md#5-6-citation-context-tool
    //= type=test
    //# - MUST support configurable context size
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // First get a citation ID using search
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
    let citation_id = search_content[0]["citations"][0]["id"].as_str().unwrap();

    // Test with different context line values
    for context_lines in [0, 1, 3, 5] {
        let result = client
            .call_tool(CallToolRequestParam {
                name: "get_citation_context".into(),
                arguments: Some(make_args(citation_id, context_lines)),
            })
            .await
            .unwrap();

        let content = parse_content(&result.content[0]);
        let context = content["context"].as_array().unwrap();
        assert_eq!(
            context.len(),
            (context_lines * 2 + 1) as usize,
            "Context should have {} lines before and after, plus the citation line",
            context_lines
        );
    }
}
