// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Tests for Section 5.8: get_prioritized_requirements Tool

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
async fn test_get_prioritized_requirements() {
    //= docs/rfcs/0001-mcp-server.md#5-8-requirement-prioritization-tool
    //= type=test
    //# - MUST implement priority sorting based on:
    //#   - Requirement level (MUST > SHOULD > MAY)
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test getting prioritized requirements
    let result = client
        .call_tool(CallToolRequestParam {
            name: "get_prioritized_requirements".into(),
            arguments: None,
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    assert!(content.is_array());

    // Verify each result has the required fields
    for requirement in content.as_array().unwrap() {
        assert!(requirement.get("full_path").is_some());
        assert!(requirement.get("level").is_some());
        assert!(requirement.get("status").is_some());
        assert!(requirement.get("todo_count").is_some());
    }
}

#[tokio::test]
async fn test_requirement_levels() {
    //= docs/rfcs/0001-mcp-server.md#5-8-requirement-prioritization-tool
    //= type=test
    //# - MUST implement priority sorting based on:
    //#   - Requirement level (MUST > SHOULD > MAY)
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    let result = client
        .call_tool(CallToolRequestParam {
            name: "get_prioritized_requirements".into(),
            arguments: None,
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    let requirements = content.as_array().unwrap();

    // Helper function to get level priority (higher number = higher priority)
    fn level_priority(level: &str) -> i32 {
        match level {
            "MUST" => 4,
            "SHOULD" => 3,
            "MAY" => 2,
            _ => 1, // unspecified
        }
    }

    // Verify requirements are sorted by level priority
    let mut prev_priority = i32::MAX;
    for req in requirements {
        let level = req["level"].as_str().unwrap();
        let current_priority = level_priority(level);
        assert!(
            current_priority <= prev_priority,
            "Requirements should be sorted by level priority"
        );
        prev_priority = current_priority;
    }
}

#[tokio::test]
async fn test_citation_status_order() {
    //= docs/rfcs/0001-mcp-server.md#5-8-requirement-prioritization-tool
    //= type=test
    //# - MUST maintain consistent ordering
    //# - MUST update priorities in real-time
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    let result = client
        .call_tool(CallToolRequestParam {
            name: "get_prioritized_requirements".into(),
            arguments: None,
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    let requirements = content.as_array().unwrap();

    // Helper function to get status priority (higher number = higher priority)
    fn status_priority(status: &str) -> i32 {
        match status {
            "partially_implemented" => 3,
            "not_started" => 2,
            "fully_implemented" => 1,
            _ => 0,
        }
    }

    // Group requirements by level and verify status ordering within each group
    let mut current_level = String::new();
    let mut prev_status_priority = i32::MAX;

    for req in requirements {
        let level = req["level"].as_str().unwrap();
        let status = req["status"].as_str().unwrap();

        // Reset priority check when level changes
        if level != current_level {
            current_level = level.to_string();
            prev_status_priority = i32::MAX;
        }

        let current_priority = status_priority(status);
        assert!(
            current_priority <= prev_status_priority,
            "Requirements within same level should be sorted by status priority"
        );
        prev_status_priority = current_priority;
    }
}

#[tokio::test]
async fn test_todo_count_order() {
    //= docs/rfcs/0001-mcp-server.md#5-8-requirement-prioritization-tool
    //= type=test
    //# - MUST maintain consistent ordering
    //# - MUST update priorities in real-time
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    let result = client
        .call_tool(CallToolRequestParam {
            name: "get_prioritized_requirements".into(),
            arguments: None,
        })
        .await
        .unwrap();

    let content = parse_content(&result.content[0]);
    let requirements = content.as_array().unwrap();

    // Group by level and status, then verify TODO count ordering
    let mut current_level = String::new();
    let mut current_status = String::new();
    let mut prev_todo_count = i64::MAX;

    for req in requirements {
        let level = req["level"].as_str().unwrap();
        let status = req["status"].as_str().unwrap();
        let todo_count = req["todo_count"].as_i64().unwrap();

        // Reset count check when level or status changes
        if level != current_level || status != current_status {
            current_level = level.to_string();
            current_status = status.to_string();
            prev_todo_count = i64::MAX;
        }

        assert!(
            todo_count <= prev_todo_count,
            "Requirements within same level and status should be sorted by TODO count"
        );
        prev_todo_count = todo_count;
    }
}
