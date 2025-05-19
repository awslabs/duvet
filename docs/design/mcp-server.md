# Duvet MCP Server Specification

This specification defines the Duvet MCP server, a component of the Duvet requirements traceability system. The server enables AI models to interact with project specifications, requirements, and code citations via the Model Context Protocol (MCP). It provides structured access to data and tools for validating and managing traceability links between requirements and their implementations in code.

---

## 1. Introduction

The Duvet MCP server facilitates requirements traceability by exposing project specifications, requirements, and citations through a standardized API. It allows AI models to query, validate, and analyze traceability data, ensuring that requirements are properly linked to code and that compliance is maintained. The server is built using MCP conventions, offering resources for data retrieval and tools for performing actions like validation and searching.

---

## 2. Architecture Overview

The server follows a client-server model, with the AI model acting as the client and the Duvet MCP server providing resources and tools via JSON-RPC methods. The architecture is designed to be hierarchical, reflecting the natural structure of specifications, sections, requirements, and citations.

The server can be started using the `stdio` interface with the `duvet mcp` command.

### Key Components:
- **Resources**: Hierarchical data endpoints for specifications, sections, requirements, and citations.
- **Tools**: Actionable operations that extend the server's functionality, such as validation, searching, and context retrieval.
- **System Prompt**: A guide provided to the AI model to explain the server's purpose, available resources, tools, and usage instructions.

---

## 3. Resource Interfaces

Resources are accessed using MCP's `resources/list` and `resources/get` methods. Each resource is organized hierarchically to support intuitive navigation.

### 3.1 Specifications

#### Path: `/specifications`
- **Method**: `resources/list`
  - **Parameters**: None
  - **Output**: A JSON array of specification objects, each containing:
    - `id` (string): Unique identifier of the specification.
    - `name` (string): Human-readable name.
    - `url` (string): URL of the specification.
    - `description` (string): Brief description.
  - **Purpose**: Lists all available specifications in the project.
  - **Example Output**:
    ```json
    [
      {"id": "spec1", "name": "RFC 2324", "url": "https://www.rfc-editor.org/rfc/rfc2324", "description": "Hyper Text Coffee Pot Control Protocol"}
    ]
    ```

#### Path: `/specifications/{spec_id}`
- **Method**: `resources/get`
  - **Parameters**:
    - `spec_id` (string): Specification identifier.
  - **Output**: A JSON object with detailed specification information.
  - **Purpose**: Retrieves details of a specific specification.
  - **Example Output**:
    ```json
    {"id": "spec1", "name": "RFC 2324", "url": "https://www.rfc-editor.org/rfc/rfc2324", "description": "Hyper Text Coffee Pot Control Protocol"}
    ```

### 3.2 Sections

#### Path: `/specifications/{spec_id}/sections`
- **Method**: `resources/list`
  - **Parameters**:
    - `spec_id` (string): Specification identifier.
  - **Output**: A JSON array of section objects, each containing:
    - `id` (string): Section identifier.
    - `title` (string): Section title.
  - **Purpose**: Lists all sections within a specification.
  - **Example Output**:
    ```json
    [
      {"id": "section-2.1", "title": "Brewing"},
      {"id": "section-2.2", "title": "Delivery"}
    ]
    ```

#### Path: `/specifications/{spec_id}/sections/{section_id}`
- **Method**: `resources/get`
  - **Parameters**:
    - `spec_id` (string): Specification identifier.
    - `section_id` (string): Section identifier.
  - **Output**: A JSON object with section details, including:
    - `id` (string): Section identifier.
    - `title` (string): Section title.
    - `content` (string): Full text of the section.
  - **Purpose**: Retrieves the content of a specific section.
  - **Example Output**:
    ```json
    {"id": "section-2.1", "title": "Brewing", "content": "The coffee pot shall brew coffee according to the specified standards..."}
    ```

### 3.3 Requirements

#### Path: `/specifications/{spec_id}/sections/{section_id}/requirements`
- **Method**: `resources/list`
  - **Parameters**:
    - `spec_id` (string): Specification identifier.
    - `section_id` (string): Section identifier.
  - **Output**: A JSON array of requirement objects, each containing:
    - `identifier` (string): Hash-based unique identifier (e.g., hex-encoded string of BLAKE3 of the text).
    - `text` (string): Full text of the requirement.
  - **Purpose**: Lists all requirements within a section.
  - **Example Output**:
    ```json
    [
      {"identifier": "abc12345", "text": "The system shall support error handling for invalid inputs."}
    ]
    ```

#### Path: `/specifications/{spec_id}/sections/{section_id}/requirements/{req_identifier}`
- **Method**: `resources/get`
  - **Parameters**:
    - `spec_id` (string): Specification identifier.
    - `section_id` (string): Section identifier.
    - `req_identifier` (string): Requirement identifier.
  - **Output**: A JSON object with requirement details, including:
    - `identifier` (string): Requirement identifier.
    - `text` (string): Full text.
    - `status` (string): Current status (e.g., "done", "in progress").
  - **Purpose**: Retrieves a specific requirement.
  - **Example Output**:
    ```json
    {"identifier": "abc12345", "text": "The system shall support error handling for invalid inputs.", "status": "done"}
    ```

### 3.4 Citations

#### Path: `/specifications/{spec_id}/sections/{section_id}/requirements/{req_identifier}/citations`
- **Method**: `resources/list`
  - **Parameters**:
    - `spec_id` (string): Specification identifier.
    - `section_id` (string): Section identifier.
    - `req_identifier` (string): Requirement identifier.
  - **Output**: A JSON array of citation objects, each containing:
    - `id` (string): Citation identifier (e.g., `src/main.rs:42`).
  - **Purpose**: Lists citations for a requirement.
  - **Example Output**:
    ```json
    [
      {"id": "src/main.rs:42"}
    ]
    ```

#### Path: `/specifications/{spec_id}/sections/{section_id}/requirements/{req_identifier}/citations/{citation_id}`
- **Method**: `resources/get`
  - **Parameters**:
    - `spec_id` (string): Specification identifier.
    - `section_id` (string): Section identifier.
    - `req_identifier` (string): Requirement identifier.
    - `citation_id` (string): Citation identifier.
  - **Output**: A JSON object with citation details, including:
    - `file_path` (string): File path.
    - `line_number` (integer): Line number.
    - `comment_text` (string): Citation comment.
    - `context_lines` (array, optional): Surrounding code lines.
  - **Purpose**: Retrieves a specific citation with optional context.
  - **Example Output**:
    ```json
    {
      "file_path": "src/main.rs",
      "line_number": 42,
      "comment_text": "//= https://www.rfc-editor.org/rfc/rfc2324#section-2.1.1",
      "context_lines": ["fn brew_coffee() {", "    //= https://...", "    println!(\"Brewing...\");"]
    }
    ```

### 3.5 Virtual Resources

#### Path: `/requirements`
- **Method**: `resources/list`
  - **Parameters**: None
  - **Output**: A JSON array of all requirements across the project, each with:
    - `full_path` (string): Full resource path.
    - `identifier` (string): Requirement identifier.
    - `text` (string): Requirement text.
  - **Purpose**: Provides a project-wide list of requirements.
  - **Example Output**:
    ```json
    [
      {"full_path": "/specifications/spec1/sections/section-2.1/requirements/abc12345", "identifier": "abc12345", "text": "The system shall..."}
    ]
    ```

#### Path: `/citations`
- **Method**: `resources/list`
  - **Parameters**: None
  - **Output**: A JSON array of all citations across the project, each with:
    - `full_path` (string): Full resource path.
    - `id` (string): Citation identifier.
  - **Purpose**: Lists all citations for global analysis.
  - **Example Output**:
    ```json
    [
      {"full_path": "/specifications/spec1/sections/section-2.1/requirements/abc12345/citations/src/main.rs:42", "id": "src/main.rs:42"}
    ]
    ```

---

## 4. Tools

Tools provide actionable operations beyond resource retrieval. Each tool is invoked using the `tools/call` method with the tool name and required arguments.

### 4.1 `validate_citation`
- **Purpose**: Validates a citation string to ensure it references an existing specification, section, and requirement.
- **Input Parameters**:
  - `citation` (string): Citation string (e.g., `//= https://www.rfc-editor.org/rfc/rfc2324#section-2.1.1`).
- **Output**: A JSON object indicating validity:
  - `{"valid": true}` if valid.
  - `{"valid": false, "error": "Section not found"}` if invalid.
- **Example**:
  - Request: `{"tool": "validate_citation", "arguments": {"citation": "//= https://www.rfc-editor.org/rfc/rfc2324#section-2.1.1"}}`
  - Response: `{"valid": true}`

### 4.2 `search_requirements`
- **Purpose**: Searches for requirements across all specifications using keywords or phrases.
- **Input Parameters**:
  - `query` (string): Search term (e.g., `"error handling"`).
- **Output**: A JSON array of matching requirements, each with:
  - `identifier` (string): Requirement identifier.
  - `full_path` (string): Full resource path.
  - `text` (string): Requirement text.
- **Example**:
  - Request: `{"tool": "search_requirements", "arguments": {"query": "error handling"}}`
  - Response: `[{"identifier": "abc12345", "full_path": "...", "text": "The system shall..."}]`

### 4.3 `get_requirement_status`
- **Purpose**: Retrieves the status of a specific requirement.
- **Input Parameters**:
  - `req_identifier` (string): Requirement identifier.
- **Output**: A JSON object with the status (e.g., `{"status": "done"}`).
- **Example**:
  - Request: `{"tool": "get_requirement_status", "arguments": {"req_identifier": "abc12345"}}`
  - Response: `{"status": "done"}`

### 4.4 `list_uncited_requirements`
- **Purpose**: Lists all requirements without citations in the codebase.
- **Input Parameters**: None
- **Output**: A JSON array of uncited requirements, each with:
  - `identifier` (string): Requirement identifier.
  - `full_path` (string): Full resource path.
  - `text` (string): Requirement text.
- **Example**:
  - Request: `{"tool": "list_uncited_requirements", "arguments": {}}`
  - Response: `[{"identifier": "def67890", "full_path": "...", "text": "The system shall..."}]`

### 4.5 `list_invalid_citations`
- **Purpose**: Lists all invalid citations in the codebase.
- **Input Parameters**: None
- **Output**: A JSON array of invalid citations, each with:
  - `file_path` (string): File path.
  - `line_number` (integer): Line number.
  - `comment_text` (string): Invalid citation.
  - `error` (string): Reason for invalidity.
- **Example**:
  - Request: `{"tool": "list_invalid_citations", "arguments": {}}`
  - Response: `[{"file_path": "src/main.rs", "line_number": 10, "comment_text": "//= invalid", "error": "URL not found"}]`

### 4.6 `get_citation_context`
- **Purpose**: Retrieves the code context surrounding a specific citation.
- **Input Parameters**:
  - `citation_id` (string): Citation identifier (e.g., `src/main.rs:42`).
  - `context_lines` (integer): Number of lines to include before and after.
- **Output**: A JSON object with:
  - `file_path` (string): File path.
  - `line_number` (integer): Line number.
  - `context` (array): Code snippet with context lines.
- **Example**:
  - Request: `{"tool": "get_citation_context", "arguments": {"citation_id": "src/main.rs:42", "context_lines": 2}}`
  - Response: `{"file_path": "src/main.rs", "line_number": 42, "context": ["line 40", "line 41", "line 42 //= ...", "line 43", "line 44"]}`

### 4.7 `resolve_spec_id`
- **Purpose**: Resolves a specification ID from a given URL.
- **Input Parameters**:
  - `url` (string): Specification URL.
- **Output**: A JSON object with:
  - `spec_id` (string): Specification ID if found.
  - `error` (string, optional): Error message if not found.
- **Example**:
  - Request: `{"tool": "resolve_spec_id", "arguments": {"url": "https://www.rfc-editor.org/rfc/rfc2324"}}`
  - Response: `{"spec_id": "spec1"}`

### 4.8 `get_prioritized_requirements`
- **Purpose**: Provides a list of all requirements ordered by priority, based on requirement level, citation status, and TODO citations.
- **Input Parameters**: None
- **Output**: A JSON array of requirement objects, each with:
  - `full_path` (string): Full resource path.
  - `level` (string): Requirement level (e.g., "MUST", "SHOULD").
  - `status` (string): Citation status ("not_started", "partially_implemented", "fully_implemented").
  - `todo_count` (integer): Number of TODO citations.
- **Sorting Logic**:
  - First by requirement level (MUST > SHOULD > MAY > unspecified).
  - Within each level, by citation status (partially_implemented > not_started > fully_implemented).
  - Within each status, by TODO count (higher count first).
- **Purpose**: Helps the model focus on the most important work by prioritizing incomplete high-priority requirements and tasks with TODOs.
- **Example**:
  - Request: `{"tool": "get_prioritized_requirements", "arguments": {}}`
  - Response: `[{"full_path": "...", "level": "MUST", "status": "partially_implemented", "todo_count": 0}, ...]`

---

## 5. System Prompt

When the Duvet MCP server is started, it provides the following system prompt to guide the AI model on how to interact with it:

**System Prompt**:
```
You are interacting with the Duvet MCP server, which provides access to project specifications, requirements, and code citations to support requirements traceability. This server enables you to link specifications and requirements to their implementations in code, ensuring compliance and validation through structured data access.

### Available Resources
You can access the following resources using the `resources/list` and `resources/get` methods:
- **Specifications**: List all specifications or get details of a specific one.
- **Sections**: List sections within a specification or get details of a specific section.
- **Requirements**: List requirements within a section or get details of a specific requirement.
- **Citations**: List citations for a requirement or get details of a specific citation.
- **Virtual Resources**: List all requirements or citations across the entire project.

### Available Tools
You can use the following tools via the `tools/call` method to perform actions:
- **Validate a citation**: Check if a citation references a valid specification, section, and requirement.
- **Search for requirements**: Find requirements using keywords or phrases.
- **Get requirement status**: Retrieve the status of a specific requirement (e.g., "done", "needs tests").
- **List uncited requirements**: Identify requirements without any citations in the code.
- **List invalid citations**: Find citations in the code that are invalid or reference non-existent resources.
- **Get citation context**: Retrieve the code surrounding a specific citation for better understanding.
- **Resolve specification ID**: Get the specification ID for a given URL to access its content.
- **Get prioritized requirements**: Retrieve a list of requirements ordered by priority, based on requirement level, citation status, and TODO citations.

### How to Interact
- **For resources**: Use `resources/list` to list items and `resources/get` to retrieve details, specifying the appropriate path (e.g., `/specifications`, `/specifications/{spec_id}/sections`).
- **For tools**: Use `tools/call` with the tool name and required arguments (e.g., `{"tool": "validate_citation", "arguments": {"citation": "//= https://..."}}`).

### Example Usage
- **To list all specifications**: Use `resources/list` with the path `/specifications`.
- **To validate a citation**: Use `tools/call` with `{"tool": "validate_citation", "arguments": {"citation": "//= https://www.rfc-editor.org/rfc/rfc2324#section-2.1.1"}}`.
- **To get prioritized requirements**: Use `tools/call` with `{"tool": "get_prioritized_requirements", "arguments": {}}`.
```

---

## 6. Implementation Considerations

- **Requirement Identification**: Requirements are identified by a hash of their text (e.g., hex-encoded string of BLAKE3) to ensure uniqueness without manual IDs.
- **Citation Validation**: The `validate_citation` tool verifies the citation’s URL, section, and requirement existence.
- **Performance**: Caching should be used for frequently accessed resources, and database queries optimized for large projects.
- **Security**: Access to sensitive data should be restricted to authorized users, potentially integrating with authentication systems.

---

## 7. Conclusion

The Duvet MCP server provides a robust, scalable, and intuitive API for interacting with project specifications, requirements, and citations. By adhering to MCP standards and offering a comprehensive set of resources and tools, it ensures efficient access to traceability data and supports the validation and management of compliance. This specification captures all updates and new features, making the server a powerful tool for developers, AI models, and project managers alike.
