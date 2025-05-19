# MCP Server Testing Infrastructure Design

## Overview

This document captures the design decisions and implementation plans for the Duvet MCP server testing infrastructure. The goal is to create a robust testing framework that allows testing individual requirements from the specification while maintaining clear traceability through duvet annotations.

## Key Design Decisions

### 1. Test Organization

We will organize tests to mirror the RFC sections, making it easy to track which requirements are being tested:

```
duvet/src/mcp/
├── tests/
│   ├── mod.rs                    # Test module setup
│   ├── section_3/               # Server Requirements
│   │   ├── mod.rs
│   │   ├── section_3_1.rs      # Protocol Requirements
│   │   ├── section_3_2.rs      # Architecture Requirements
│   │   ├── section_3_3.rs      # Integration Requirements
│   │   ├── section_3_4.rs      # Data Management Requirements
│   │   ├── section_3_5.rs      # Concurrency Requirements
│   │   └── section_3_6.rs      # Performance Requirements
│   ├── section_4/              # Resource Requirements
│   │   ├── mod.rs
│   │   ├── section_4_1.rs     # General Resource Requirements
│   │   ├── section_4_2.rs     # Specification Resources
│   │   ├── section_4_3.rs     # Section Resources
│   │   ├── section_4_4.rs     # Requirement Resources
│   │   ├── section_4_5.rs     # Citation Resources
│   │   └── section_4_6.rs     # Virtual Resources
│   └── section_5/             # Tool Requirements
       ├── mod.rs
       ├── section_5_1.rs      # Citation Validation Tool
       ├── section_5_2.rs      # Requirement Search Tool
       └── ...
```

This structure provides:
- Clear mapping between tests and specification sections
- Easy tracking of test coverage
- Logical organization following the specification
- Simple navigation between specification and tests

### 2. Server Communication

Instead of spawning actual processes, we'll use tokio's io::duplex module for server communication:

```rust
pub struct McpServer {
    /// The client side of the duplex connection
    client: DuplexStream,
    /// Request ID counter
    next_id: u64,
}
```

Benefits:
- More efficient than process spawning
- Better control over server lifecycle
- Easier async operation testing
- Improved testability for edge cases
- Faster test execution
- Ability to share test context between tests

### 3. Test Context Management

Each test will run in an isolated context with its own temporary file system:

```rust
pub struct TestContext {
    /// Root directory for this test
    root: PathBuf,
    /// Temporary directory that will be cleaned up
    _temp_dir: TempDir,
}
```

This provides:
- Isolation between tests
- Clean state for each test
- Ability to create test specifications and source files
- Automatic cleanup after tests complete

### 4. Test Implementation Style

Tests will be implemented with direct, inline assertions:

```rust
#[tokio::test]
async fn test_section_3_1_protocol_requirements() {
    //= docs/rfcs/0001-mcp-server.md#31-protocol-requirements
    //= type=test
    //# The server:
    //# - MUST implement the Model Context Protocol (MCP)
    let ctx = Arc::new(TestContext::new().unwrap());
    
    let mut server = McpServer::start(ctx).await.unwrap();
    
    let response = server.call("resources/list", json!({
        "path": "/specifications"
    })).await.unwrap();
    
    assert!(response.is_success());
    // ... assertions
}
```

Benefits:
- Clear test flow
- Immediate assertions
- Better error context
- Easier debugging
- More flexible for edge cases

## Implementation Plan

1. Core Infrastructure
   - Implement TestContext for managing test environments
   - Create McpServer with tokio::io::duplex support
   - Add JSON-RPC request/response handling
   - Set up test helper functions and macros

2. Test Organization
   - Create directory structure mirroring RFC sections
   - Set up mod.rs files for each section
   - Add test utilities module

3. Initial Tests
   - Start with Protocol Requirements (Section 3.1)
   - Add Resource Requirements tests (Section 4)
   - Implement Tool Requirements tests (Section 5)

4. Helper Functions
   - Add specification creation helpers
   - Create source file utilities
   - Implement assertion macros

5. Advanced Features
   - Add support for concurrent request testing
   - Implement error injection capabilities
   - Add performance test helpers

## Testing Workflow

1. Create test context
2. Set up test files (specifications, source files)
3. Start MCP server
4. Send requests and verify responses
5. Make assertions about server behavior
6. Context cleanup happens automatically

## Example Test Flow

```rust
// 1. Create context
let ctx = Arc::new(TestContext::new().unwrap());

// 2. Set up test files
ctx.create_spec("test", r#"
    # Section 1
    The system MUST work.
"#).unwrap();

// 3. Start server
let mut server = McpServer::start(ctx).await.unwrap();

// 4. Send request
let response = server.call("resources/list", json!({
    "path": "/specifications"
})).await.unwrap();

// 5. Verify response
assert!(response.is_success());
assert_json_matches!(response.result, json!([
    {
        "id": String,
        "name": String,
        "url": String,
        "description": String
    }
]));
```

## Next Steps

1. Implement core TestContext and McpServer structs
2. Set up test directory structure
3. Create initial test for Section 3.1
4. Add helper functions as needed
5. Expand test coverage to other sections

## Future Considerations

1. Performance testing capabilities
2. Concurrent request handling
3. Error injection framework
4. Test coverage reporting
5. Integration with existing CI pipeline
