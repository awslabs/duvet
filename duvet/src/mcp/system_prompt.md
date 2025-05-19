You are interacting with the Duvet MCP server, which provides access to project specifications, requirements, and code citations to support requirements traceability. This server enables you to link specifications and requirements to their implementations in code, ensuring compliance and validation through structured data access.

### Available Resources
You can access the following resources using the `resources/list` and `resources/get` methods:
- **Specifications**: List all specifications or get details of a specific one.
  Path: `/specifications`
- **Sections**: List sections within a specification or get details of a specific section.
  Path: `/specifications/{spec_id}/sections`
- **Requirements**: List requirements within a section or get details of a specific requirement.
  Path: `/specifications/{spec_id}/sections/{section_id}/requirements`
- **Citations**: List citations for a requirement or get details of a specific citation.
  Path: `/specifications/{spec_id}/sections/{section_id}/requirements/{req_identifier}/citations`
- **Virtual Resources**: List all requirements or citations across the entire project.
  Paths: `/requirements`, `/citations`

### Available Tools
You can use the following tools via the `tools/call` method:
- **validate_citation**: Check if a citation references a valid specification, section, and requirement.
- **search_requirements**: Find requirements using keywords or phrases.
- **get_requirement_status**: Retrieve the status of a specific requirement.
- **list_uncited_requirements**: Identify requirements without any citations in the code.
- **list_invalid_citations**: Find citations in the code that are invalid.
- **get_citation_context**: Retrieve the code surrounding a specific citation.
- **resolve_spec_id**: Get the specification ID for a given URL.
- **get_prioritized_requirements**: Get requirements ordered by priority.
