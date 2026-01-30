# Implementation Plan: Source Blob Link

## Overview

This implementation adds per-source blob link support to Duvet's configuration. The changes flow through the config schema, internal config structs, source file processing, annotation creation, JSON report generation, and the React frontend.

## Tasks

- [x] 1. Update config schema to support blob-link on source blocks
  - [x] 1.1 Add `blob_link` field to `v0_4_0::Source` struct
    - Add `#[serde(default, rename = "blob-link")] pub blob_link: Option<TemplatedString>` field
    - Ensure `schemars::JsonSchema` derive generates correct schema
    - _Requirements: 1.1, 1.2, 1.3_

  - [x] 1.2 Update `load_sources()` to propagate blob_link to internal config
    - Pass `source.blob_link.as_ref().map(From::from)` when creating `config::Source`
    - _Requirements: 2.1, 2.2_

  - [x] 1.3 Add `blob_link` field to `config::Source` struct
    - Add `pub blob_link: Option<Arc<str>>` field to the internal Source struct
    - _Requirements: 2.3_

- [x] 2. Checkpoint - Verify config parsing works
  - Run `cargo xtask test` to ensure existing tests pass
  - Manually test parsing a config with `blob-link` on a source block

- [x] 3. Propagate blob_link through source file processing
  - [x] 3.1 Update `SourceFile::Text` variant to include blob_link
    - Add `blob_link: Option<Arc<str>>` field to the `Text` variant
    - Update all places that construct `SourceFile::Text` to include blob_link
    - _Requirements: 3.1, 3.3_

  - [x] 3.2 Update project sources collection to pass blob_link
    - Modify the code that creates `SourceFile` instances from config sources
    - Pass the source's `blob_link` to `SourceFile::Text`
    - _Requirements: 3.1_

- [x] 4. Add blob_link to Annotation struct
  - [x] 4.1 Add `blob_link` field to `Annotation` struct
    - Add `pub blob_link: Option<Arc<str>>` field
    - _Requirements: 4.2_

  - [x] 4.2 Update annotation extraction to inherit blob_link from source
    - Modify `SourceFile::annotations()` to pass blob_link to extracted annotations
    - Update comment extraction to accept and propagate blob_link
    - _Requirements: 3.2, 4.1_

- [x] 5. Checkpoint - Verify annotation extraction works
  - Run `cargo xtask test` to ensure existing tests pass
  - Verify annotations have correct blob_link values

- [x] 6. Update JSON report generation
  - [x] 6.1 Serialize blob_link field in annotation JSON output
    - Add conditional serialization of `blob_link` field in `json.rs`
    - Only include field when annotation has a blob_link value
    - _Requirements: 5.1, 5.2_

  - [ ]* 6.2 Write property test for JSON serialization
    - **Property 3: JSON Serialization Consistency**
    - **Validates: Requirements 5.1**

- [x] 7. Update React frontend to use per-annotation blob links
  - [x] 7.1 Modify `createBlobLinker` function in `result.js`
    - Update to check `anno.blob_link` before falling back to global
    - Handle case where neither is available (return null href)
    - _Requirements: 6.1, 6.2, 6.3_

  - [ ]* 7.2 Write unit test for frontend blob link resolution
    - **Property 4: Frontend Blob Link Resolution**
    - **Validates: Requirements 6.1, 6.2, 6.3**

- [x] 8. Update JSON schema file
  - [x] 8.1 Regenerate `config/v0.4.0.json` schema
    - Run the schema generation test to update the JSON schema file
    - Verify `blob-link` property is included on Source definition
    - _Requirements: 7.1, 7.2, 7.3_

- [x] 9. Update documentation
  - [x] 9.1 Update `guide/src/example-config.toml`
    - Add example of `blob-link` on a `[[source]]` block
    - _Requirements: 9.1_

  - [x] 9.2 Update `guide/src/config.md` with explanation
    - Add section explaining per-source blob links
    - Explain override behavior and use cases
    - _Requirements: 9.2, 9.3_

- [ ] 10. Add integration test
  - [ ] 10.1 Create integration test config with per-source blob links
    - Add new test config in `integration/` directory
    - Include sources with different blob-link values
    - _Requirements: 8.1, 8.2_

  - [ ]* 10.2 Write property test for backward compatibility
    - **Property 5: Backward Compatibility**
    - **Validates: Requirements 8.1, 8.2**

- [ ] 11. Final checkpoint - Run full test suite
  - Run `cargo xtask test` to verify all tests pass
  - Run `cargo xtask checks` to verify clippy and rustfmt pass
  - Ensure all tests pass, ask the user if questions arise

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- Each task references specific requirements for traceability
- The implementation follows the data flow: config → source file → annotation → report
- Property tests validate universal correctness properties
- Unit tests validate specific examples and edge cases
