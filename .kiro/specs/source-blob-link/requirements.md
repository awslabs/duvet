# Requirements Document: Source Blob Link

## Overview

This document specifies the requirements for adding per-source blob link support to Duvet's configuration. This feature enables different source patterns to have their own blob links, which is essential for multi-package report merging where source files may reside in different repositories.

## Glossary

- **Blob Link**: A URL prefix used to generate links to source files in a repository (e.g., `https://github.com/org/repo/blob/main`)
- **Source Block**: A `[[source]]` configuration section in `.duvet/config.toml` that defines a pattern for scanning source files
- **Global Blob Link**: The `report.html.blob-link` configuration that applies to all sources by default
- **Annotation**: A special comment in source code that links to a specification section
- **JSON Report**: The JSON output file containing all annotations and their metadata
- **HTML Report**: The HTML output file that renders the compliance report with clickable source links

---

## Requirement 1: Source Configuration Schema

**User Story:** As a Duvet user, I want to specify a blob link for each source pattern, so that annotations from different packages can link to their correct repositories.

### Acceptance Criteria

1. WHEN a `[[source]]` block includes a `blob-link` field THEN THE Schema SHALL parse and store the value as an optional templated string
2. WHEN a `[[source]]` block omits the `blob-link` field THEN THE Schema SHALL set the blob link to None
3. WHEN the config file is validated against the JSON schema THEN THE Schema SHALL accept `blob-link` as a valid optional property on source blocks

---

## Requirement 2: Internal Config Propagation

**User Story:** As a Duvet developer, I want the blob link to propagate through the internal config structures, so that it is available during annotation processing.

### Acceptance Criteria

1. WHEN `load_sources()` processes a source with `blob-link` THEN THE Config SHALL create a `config::Source` with `blob_link` set to the parsed value
2. WHEN `load_sources()` processes a source without `blob-link` THEN THE Config SHALL create a `config::Source` with `blob_link` set to None
3. THE `config::Source` struct SHALL include a `blob_link: Option<Arc<str>>` field

---

## Requirement 3: Source File Blob Link

**User Story:** As a Duvet developer, I want source files to carry their blob link, so that annotations extracted from them inherit the correct link.

### Acceptance Criteria

1. WHEN a `SourceFile::Text` is created THEN THE SourceFile SHALL store the blob link from its source configuration
2. WHEN annotations are extracted from a source file THEN THE Annotation SHALL inherit the blob link from the source file
3. THE `SourceFile::Text` variant SHALL include a `blob_link: Option<Arc<str>>` field

---

## Requirement 4: Annotation Blob Link Storage

**User Story:** As a Duvet developer, I want each annotation to store its blob link, so that report generation can use the correct link per annotation.

### Acceptance Criteria

1. WHEN an annotation is created from a source file THEN THE Annotation SHALL store the source file's blob link
2. THE `Annotation` struct SHALL include a `blob_link: Option<Arc<str>>` field
3. WHEN an annotation's blob link is None THEN THE Report generation SHALL fall back to the global blob link

---

## Requirement 5: JSON Report Output

**User Story:** As a Duvet user, I want the JSON report to include per-annotation blob links, so that the HTML frontend can generate correct source links.

### Acceptance Criteria

1. WHEN an annotation has a blob link THEN THE JSON Report SHALL include a `blob_link` field in the annotation object
2. WHEN an annotation has no blob link THEN THE JSON Report SHALL omit the `blob_link` field from the annotation object
3. THE JSON Report SHALL continue to include the global `blob_link` field at the top level for backward compatibility

---

## Requirement 6: Frontend Blob Link Resolution

**User Story:** As a Duvet user viewing an HTML report, I want source file links to use the correct blob link for each annotation, so that I can navigate to the correct repository.

### Acceptance Criteria

1. WHEN generating a source link for an annotation with a `blob_link` field THEN THE Frontend SHALL use the annotation's blob link
2. WHEN generating a source link for an annotation without a `blob_link` field THEN THE Frontend SHALL fall back to the global blob link
3. WHEN neither annotation nor global blob link is available THEN THE Frontend SHALL display the source path without a hyperlink

---

## Requirement 7: JSON Schema Update

**User Story:** As a Duvet user, I want the JSON schema to validate my config correctly, so that I get helpful errors when my config is malformed.

### Acceptance Criteria

1. WHEN the JSON schema is generated THEN THE Schema SHALL include `blob-link` as an optional property on the Source definition
2. THE `blob-link` property SHALL have the same type as `report.html.blob-link` (TemplatedString or null)
3. THE JSON schema file `config/v0.4.0.json` SHALL be updated to reflect the new property

---

## Requirement 8: Backward Compatibility

**User Story:** As an existing Duvet user, I want my current configs to continue working, so that I don't have to update all my projects immediately.

### Acceptance Criteria

1. WHEN a config file has no `blob-link` fields on any source blocks THEN THE System SHALL parse successfully and use the global blob link for all annotations
2. WHEN a config file has `blob-link` on some source blocks but not others THEN THE System SHALL use the source-level blob link where specified and fall back to global elsewhere
3. THE System SHALL not require any changes to existing valid config files

---

## Requirement 9: Documentation Update

**User Story:** As a Duvet user, I want the documentation to explain the per-source blob link feature, so that I can understand how to configure it for my multi-package projects.

### Acceptance Criteria

1. WHEN a user reads the configuration guide THEN THE Documentation SHALL include an example of `blob-link` on a `[[source]]` block
2. THE Documentation SHALL explain that source-level `blob-link` overrides the global `report.html.blob-link`
3. THE Documentation SHALL explain the use case for per-source blob links (multi-package/multi-repo scenarios)
