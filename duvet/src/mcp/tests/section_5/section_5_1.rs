// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Tests for Section 5.1: validate_citation Tool

use crate::mcp::tests::{Test, TestContext};
use rmcp::model::{Annotated, CallToolRequestParam, Content, RawContent};
use serde_json::{Map, Value};
use std::sync::Arc;

fn make_args(citation: &str) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert("citation".to_string(), Value::String(citation.to_string()));
    map
}

fn parse_content(content: &Annotated<RawContent>) -> Value {
    match &content.raw {
        RawContent::Text(text) => serde_json::from_str(&text.text).unwrap(),
        _ => panic!("Expected text content"),
    }
}

#[tokio::test]
async fn test_validate_citation() {
    //= docs/rfcs/0001-mcp-server.md#5.1
    //= type=test
    //# - MUST verify citation URL format
    //# - MUST validate specification references
    //# - MUST validate section references
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test valid citation
    let result = client
        .call_tool(CallToolRequestParam {
            name: "validate_citation".into(),
            arguments: Some(make_args("//= docs/rfcs/0001-mcp-server.md#5.1")),
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    assert_eq!(content["valid"].as_bool().unwrap(), true);

    // Test invalid specification
    let result = client
        .call_tool(CallToolRequestParam {
            name: "validate_citation".into(),
            arguments: Some(make_args("//= docs/rfcs/non-existent.md#section-1")),
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    assert_eq!(content["valid"].as_bool().unwrap(), false);
    assert!(
        content["error"]
            .as_str()
            .unwrap()
            .contains("Specification not found")
    );

    // Test invalid section
    let result = client
        .call_tool(CallToolRequestParam {
            name: "validate_citation".into(),
            arguments: Some(make_args("//= docs/rfcs/0001-mcp-server.md#non-existent")),
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    assert_eq!(content["valid"].as_bool().unwrap(), false);
    assert!(
        content["error"]
            .as_str()
            .unwrap()
            .contains("Section not found")
    );

    // Test invalid requirement text
    let result = client
        .call_tool(CallToolRequestParam {
            name: "validate_citation".into(),
            arguments: Some(make_args(
                "//= docs/rfcs/0001-mcp-server.md#5.1\n//# Non-existent requirement",
            )),
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    assert_eq!(content["valid"].as_bool().unwrap(), false);
    assert!(
        content["error"]
            .as_str()
            .unwrap()
            .contains("Requirement not found")
    );
}

#[tokio::test]
async fn test_citation_format() {
    //= docs/rfcs/0001-mcp-server.md#5.1
    //= type=test
    //# - MUST verify citation URL format
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test invalid URL syntax
    let result = client
        .call_tool(CallToolRequestParam {
            name: "validate_citation".into(),
            arguments: Some(make_args("//= not-a-url#section")),
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    assert_eq!(content["valid"].as_bool().unwrap(), false);
    assert!(content["error"].as_str().unwrap().contains("Invalid URL"));

    // Test invalid section anchor format
    let result = client
        .call_tool(CallToolRequestParam {
            name: "validate_citation".into(),
            arguments: Some(make_args("//= docs/rfcs/0001-mcp-server.md#invalid anchor")),
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    assert_eq!(content["valid"].as_bool().unwrap(), false);
    assert!(
        content["error"]
            .as_str()
            .unwrap()
            .contains("Invalid section anchor")
    );
}

#[tokio::test]
async fn test_citation_response() {
    //= docs/rfcs/0001-mcp-server.md#5.1
    //= type=test
    //# - MUST provide detailed error messages
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test valid citation response
    let result = client
        .call_tool(CallToolRequestParam {
            name: "validate_citation".into(),
            arguments: Some(make_args("//= docs/rfcs/0001-mcp-server.md#5.1")),
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    assert!(content.get("valid").is_some());
    assert_eq!(content["valid"].as_bool().unwrap(), true);

    // Test invalid citation response
    let result = client
        .call_tool(CallToolRequestParam {
            name: "validate_citation".into(),
            arguments: Some(make_args("//= invalid")),
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    assert!(content.get("valid").is_some());
    assert_eq!(content["valid"].as_bool().unwrap(), false);
    assert!(content.get("error").is_some());
    assert!(!content["error"].as_str().unwrap().is_empty());
}
