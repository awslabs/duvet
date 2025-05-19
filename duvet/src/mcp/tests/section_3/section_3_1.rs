// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Tests for Section 3.1: Protocol Requirements

use crate::mcp::tests::{Test, TestContext};
use std::sync::Arc;

#[tokio::test]
async fn test_mcp_protocol_implementation() {
    crate::tracing::init();

    //= docs/rfcs/0001-mcp-server.md#31-protocol-requirements
    //= type=test
    //# The server:
    //# - MUST implement the Model Context Protocol (MCP)
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test basic MCP protocol functionality by listing specifications
    let resources = client.list_all_resources().await.unwrap();
    assert!(!resources.is_empty());
}

#[tokio::test]
async fn test_json_rpc_support() {
    //= docs/rfcs/0001-mcp-server.md#31-protocol-requirements
    //= type=test
    //# The server:
    //# - MUST support JSON-RPC method calls
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test JSON-RPC request format by listing resources
    let resources = client.list_all_resources().await.unwrap();
    assert!(!resources.is_empty());

    // Test JSON-RPC error response by calling an invalid method
    let result = client
        .get_prompt(rmcp::model::GetPromptRequestParam {
            name: "nonexistent".into(),
            arguments: None,
        })
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_stdio_interface() {
    //= docs/rfcs/0001-mcp-server.md#31-protocol-requirements
    //= type=test
    //# The server:
    //# - MUST provide a stdio interface
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test that we can communicate over stdio by listing resources
    let resources = client.list_all_resources().await.unwrap();
    assert!(!resources.is_empty());
}

#[tokio::test]
async fn test_startup_command() {
    //= docs/rfcs/0001-mcp-server.md#31-protocol-requirements
    //= type=test
    //# The server:
    //# - MUST support the `duvet mcp` command for startup
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Test that the server starts and responds to requests
    let resources = client.list_all_resources().await.unwrap();
    assert!(!resources.is_empty());
}

#[tokio::test]
async fn test_graceful_shutdown() {
    //= docs/rfcs/0001-mcp-server.md#31-protocol-requirements
    //= type=test
    //# The server:
    //# - SHOULD support graceful shutdown
    let ctx = Arc::new(TestContext::new().unwrap());
    let client = Test::start(ctx).await.unwrap();

    // Send a shutdown request and verify it succeeds
    client.cancel().await.unwrap();
}
