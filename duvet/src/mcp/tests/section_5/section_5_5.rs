// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Tests for Section 5.5: list_invalid_citations Tool

use crate::mcp::tests::{Test, TestContext};
use rmcp::model::{Annotated, CallToolRequestParam, Content, RawContent};
use serde_json::{Map, Value};
use std::sync::Arc;

fn parse_content(content: &Annotated<RawContent>) -> Value {
    match &content.raw {
        RawContent::Text(text) => serde_json::from_str(&text.text).unwrap(),
        _ => panic!("Expected text content"),
    }
}

#[tokio::test]
async fn test_list_invalid_citations() {
    //= docs/rfcs/0001-mcp-server.md#5-5-invalid-citations-tool
    //= type=test
    //# - MUST detect broken references
    //# - MUST identify malformed citations
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test listing invalid citations
    let result = client
        .call_tool(CallToolRequestParam {
            name: "list_invalid_citations".into(),
            arguments: None,
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    assert!(content.is_array());

    // Verify each result has the required fields
    for citation in content.as_array().unwrap() {
        assert!(citation.get("file_path").is_some());
        assert!(citation.get("line_number").is_some());
        assert!(citation.get("comment_text").is_some());
        assert!(citation.get("error").is_some());
    }
}

#[tokio::test]
async fn test_invalid_citations_format() {
    //= docs/rfcs/0001-mcp-server.md#5-5-invalid-citations-tool
    //= type=test
    //# - MUST provide error context
    //# - MUST suggest corrections
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    let result = client
        .call_tool(CallToolRequestParam {
            name: "list_invalid_citations".into(),
            arguments: None,
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    assert!(content.is_array());

    // Check first result has all required fields and correct types
    if let Some(first_result) = content.as_array().unwrap().first() {
        assert!(first_result.get("file_path").unwrap().is_string());
        assert!(first_result.get("line_number").unwrap().is_number());
        assert!(first_result.get("comment_text").unwrap().is_string());
        assert!(first_result.get("error").unwrap().is_string());
    }
}

#[tokio::test]
async fn test_verify_invalid() {
    //= docs/rfcs/0001-mcp-server.md#5-5-invalid-citations-tool
    //= type=test
    //# - MUST detect broken references
    //# - MUST identify malformed citations
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Get invalid citations
    let result = client
        .call_tool(CallToolRequestParam {
            name: "list_invalid_citations".into(),
            arguments: None,
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    let invalid = content.as_array().unwrap();

    // For each invalid citation, verify it is actually invalid using validate_citation
    for citation in invalid {
        let comment_text = citation["comment_text"].as_str().unwrap();

        // Try to validate this citation
        let validate_result = client
            .call_tool(CallToolRequestParam {
                name: "validate_citation".into(),
                arguments: Some({
                    let mut map = Map::new();
                    map.insert(
                        "citation".to_string(),
                        Value::String(comment_text.to_string()),
                    );
                    map
                }),
            })
            .await
            .unwrap();

        let validate_content = parse_content(&validate_result.content[0]);
        assert_eq!(validate_content["valid"].as_bool().unwrap(), false);
        assert!(validate_content.get("error").is_some());
    }
}
