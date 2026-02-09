# Requirements Document: Stable Annotation Identifiers

## Overview

This document specifies the requirements for adding stable, content-based annotation identifiers to Duvet. This feature replaces the ephemeral sequential `usize` IDs with deterministic IDs derived from annotation content, enabling future report merging across multiple packages.

## Glossary

- **Stable ID**: A 16-character hex string derived from annotation content using FNV-1a hashing
- **Sequential ID**: The current `usize` ID assigned by array position during `reference_map()` construction
- **Composite Key**: The tuple `(source_path, anno_line, target_path)` used to generate stable IDs
- **FNV-1a**: Fowler-Noll-Vo hash function variant 1a, a non-cryptographic hash function
- **Phase-in**: Strategy of adding stable IDs as inert metadata while maintaining v1 compatibility
- **AnnotationWithId**: Struct that pairs an annotation with its assigned IDs

---

## Requirement 1: FNV-1a Hash Implementation

**User Story:** As a Duvet developer, I want a deterministic hash function with no external dependencies, so that stable IDs can be computed reliably across all environments.

### Acceptance Criteria

1. WHEN `fnv1a_64()` is called with a byte slice THEN THE Function SHALL return a 64-bit unsigned integer
2. WHEN `fnv1a_64()` is called with the same input multiple times THEN THE Function SHALL return the same output each time
3. WHEN `fnv1a_64()` is called with an empty byte slice THEN THE Function SHALL return the FNV offset basis (0xcbf29ce484222325)
4. THE `fnv1a_64()` implementation SHALL use the standard FNV-1a constants (offset basis: 0xcbf29ce484222325, prime: 0x100000001b3)

---

## Requirement 2: Stable ID Generation

**User Story:** As a Duvet developer, I want to generate deterministic IDs from annotation content, so that the same annotation produces the same ID across independent report runs.

### Acceptance Criteria

1. WHEN `stable_annotation_id()` is called with an annotation THEN THE Function SHALL return a 16-character lowercase hex string
2. WHEN `stable_annotation_id()` is called with the same annotation multiple times THEN THE Function SHALL return the same ID each time
3. THE stable ID SHALL be derived from the composite key: `(source_path, anno_line, target_path)`
4. THE composite key components SHALL be separated by null bytes (`\0`) before hashing
5. WHEN two annotations have identical composite keys THEN THE Function SHALL return identical stable IDs
6. WHEN two annotations have different composite keys THEN THE Function SHALL return different stable IDs with high probability

---

## Requirement 3: Composite Key Design

**User Story:** As a Duvet developer, I want the composite key to uniquely identify annotations within a package, so that stable IDs are collision-free for practical use cases.

### Acceptance Criteria

1. THE composite key SHALL include `annotation.source.to_string_lossy()` as the source path component
2. THE composite key SHALL include `annotation.anno_line` as the line number component
3. THE composite key SHALL include `annotation.target_path()` as the target path component
4. THE composite key SHALL NOT include `target_section` (redundant given source+line uniqueness)
5. THE composite key SHALL NOT include `anno_type` (redundant given source+line uniqueness)
6. THE composite key SHALL NOT include `quote` (can be long; not needed for uniqueness)

---

## Requirement 4: AnnotationWithId Extension

**User Story:** As a Duvet developer, I want AnnotationWithId to store both sequential and stable IDs, so that we can phase in stable IDs while maintaining backward compatibility.

### Acceptance Criteria

1. THE `AnnotationWithId` struct SHALL include a `stable_id: String` field
2. THE `stable_id` field SHALL store the content-derived 16-character hex ID
3. THE existing `id: usize` field SHALL continue to store the sequential ID
4. WHEN `AnnotationWithId` is constructed THEN THE stable_id SHALL be computed from the annotation content

---

## Requirement 5: Reference Map Construction

**User Story:** As a Duvet developer, I want stable IDs computed during reference map construction, so that all annotations have stable IDs available for report generation.

### Acceptance Criteria

1. WHEN `reference_map()` processes an annotation THEN THE Function SHALL compute its stable ID using `stable_annotation_id()`
2. WHEN `reference_map()` creates an `AnnotationWithId` THEN THE Function SHALL populate both `id` and `stable_id` fields
3. THE sequential `id` assignment SHALL remain unchanged (enumeration order from BTreeSet)
4. WHEN the same annotation set is processed multiple times THEN THE Function SHALL produce identical stable IDs for each annotation

---

## Requirement 6: JSON Report Output

**User Story:** As a Duvet user, I want stable IDs included in the JSON report, so that I can validate determinism and prepare for v2 format migration.

### Acceptance Criteria

1. WHEN an annotation is serialized to JSON THEN THE Report SHALL include a `stable_id` field with the 16-character hex value
2. THE `stable_id` field SHALL appear in the annotation object alongside existing fields
3. THE JSON output format SHALL remain v1 compatible (frontend ignores unknown fields)
4. WHEN the same source is processed twice THEN THE Report SHALL produce identical `stable_id` values for each annotation

---

## Requirement 7: Stable ID Format Validation

**User Story:** As a Duvet developer, I want stable IDs to have a consistent format, so that they can be reliably parsed and validated.

### Acceptance Criteria

1. THE stable ID SHALL be exactly 16 characters long
2. THE stable ID SHALL contain only lowercase hexadecimal characters (0-9, a-f)
3. THE stable ID SHALL include leading zeros when the hash value is less than 2^60
4. THE stable ID format SHALL match the regex pattern `^[0-9a-f]{16}$`

---

## Requirement 8: Backward Compatibility

**User Story:** As an existing Duvet user, I want my current workflows to continue working unchanged, so that I can adopt stable IDs incrementally.

### Acceptance Criteria

1. WHEN a v1 JSON consumer parses the report THEN THE Consumer SHALL ignore the `stable_id` field (unknown field handling)
2. THE frontend SHALL continue using sequential integer IDs for annotation lookup
3. THE `statuses` map SHALL continue using sequential integer IDs as keys
4. WHEN existing integration tests run THEN THE Tests SHALL pass without modification (except snapshot updates)

---

## Requirement 9: Determinism Guarantee

**User Story:** As a Duvet user preparing for multi-package merge, I want stable IDs to be deterministic from source content, so that independent report runs can be merged reliably.

### Acceptance Criteria

1. WHEN the same source files are processed by two independent Duvet runs THEN THE Runs SHALL produce identical stable IDs for each annotation
2. THE stable ID SHALL NOT depend on processing order, system time, or random values
3. THE stable ID SHALL NOT depend on fields that may vary between runs (e.g., absolute paths if relative paths are used)
4. WHEN source files change THEN THE stable IDs for unchanged annotations SHALL remain the same (if their composite key is unchanged)

---

## Requirement 10: Testing and Validation

**User Story:** As a Duvet developer, I want comprehensive tests for stable ID generation, so that I can ensure correctness and catch regressions.

### Acceptance Criteria

1. THE implementation SHALL include unit tests for `fnv1a_64()` with known test vectors
2. THE implementation SHALL include unit tests for `stable_annotation_id()` with various annotation configurations
3. THE implementation SHALL include a property test verifying determinism (same input → same output)
4. THE implementation SHALL include an integration test verifying cross-run consistency
