// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Tests for Section 5.4: list_uncited_requirements Tool

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
async fn test_list_uncited_requirements() {
    //= docs/rfcs/0001-mcp-server.md#5.4
    //= type=test
    //# - MUST identify requirements without citations
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test listing uncited requirements
    let result = client
        .call_tool(CallToolRequestParam {
            name: "list_uncited_requirements".into(),
            arguments: None,
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    assert!(content.is_array());

    // Verify each result has the required fields
    for requirement in content.as_array().unwrap() {
        assert!(requirement.get("identifier").is_some());
        assert!(requirement.get("full_path").is_some());
        assert!(requirement.get("text").is_some());
    }
}

#[tokio::test]
async fn test_uncited_requirements_format() {
    //= docs/rfcs/0001-mcp-server.md#5.4
    //= type=test
    //# - MUST provide requirement context
    //# - MUST include requirement metadata
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    let result = client
        .call_tool(CallToolRequestParam {
            name: "list_uncited_requirements".into(),
            arguments: None,
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

#[tokio::test]
async fn test_verify_uncited() {
    //= docs/rfcs/0001-mcp-server.md#5.4
    //= type=test
    //# - MUST identify requirements without citations
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Get uncited requirements
    let result = client
        .call_tool(CallToolRequestParam {
            name: "list_uncited_requirements".into(),
            arguments: None,
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    let uncited = content.as_array().unwrap();

    // For each uncited requirement, verify it has no citations
    for requirement in uncited {
        let req_id = requirement["identifier"].as_str().unwrap();

        // Try to get citations for this requirement using search
        let search_result = client
            .call_tool(CallToolRequestParam {
                name: "search_requirements".into(),
                arguments: Some({
                    let mut map = Map::new();
                    map.insert("query".to_string(), Value::String(req_id.to_string()));
                    map
                }),
            })
            .await
            .unwrap();

        let search_content = parse_content(&search_result.content[0]);
        let found_req = search_content
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["identifier"].as_str().unwrap() == req_id)
            .unwrap();

        // Verify this requirement has no citations
        assert_eq!(
            found_req
                .get("citations")
                .map_or(0, |c| c.as_array().unwrap().len()),
            0
        );
    }
}
