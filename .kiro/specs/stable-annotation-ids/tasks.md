# Implementation Plan: Stable Annotation Identifiers

## Overview

This implementation adds content-based deterministic IDs to annotations using FNV-1a hashing. The changes flow through the annotation module (hash function + ID generation), the reference map construction, and JSON report output. The stable IDs are added as inert metadata alongside existing sequential IDs for v1 compatibility.

## Tasks

- [x] 1. Implement FNV-1a hash function
  - [x] 1.1 Add `fnv1a_64()` function to `annotation.rs`
    - Implement FNV-1a 64-bit hash with standard constants
    - Use `const` for FNV_OFFSET_BASIS (0xcbf29ce484222325) and FNV_PRIME (0x100000001b3)
    - _Requirements: 1.1, 1.2, 1.3, 1.4_

  - [x] 1.2 Write unit tests for `fnv1a_64()`
    - Test empty input returns FNV offset basis
    - Test known FNV-1a test vectors
    - Test determinism (same input → same output)
    - _Requirements: 1.2, 1.3, 10.1_

- [x] 2. Implement stable ID generation
  - [x] 2.1 Add `stable_annotation_id()` function to `annotation.rs`
    - Build composite key: `"{source}\0{anno_line}\0{target_path}"`
    - Hash with `fnv1a_64()` and format as 16-char lowercase hex
    - _Requirements: 2.1, 2.3, 2.4, 3.1, 3.2, 3.3_

  - [x] 2.2 Write unit tests for `stable_annotation_id()`
    - Test output format (16 lowercase hex chars)
    - Test determinism (same annotation → same ID)
    - Test different annotations produce different IDs
    - _Requirements: 2.1, 2.2, 2.5, 2.6, 10.2_

  - [x] 2.3 Write property test for stable ID determinism
    - **Property 1: Hash Determinism**
    - **Validates: Requirements 1.2, 2.2, 9.2**

- [x] 3. Checkpoint - Verify hash functions work
  - Run `cargo xtask test` to ensure tests pass
  - Ensure all tests pass, ask the user if questions arise

- [x] 4. Extend AnnotationWithId struct
  - [x] 4.1 Add `stable_id: String` field to `AnnotationWithId`
    - Add field to struct definition in `annotation.rs`
    - Update `PartialEq`, `PartialOrd`, `Eq`, `Ord`, `Hash` derives if needed
    - _Requirements: 4.1, 4.2, 4.3_

  - [x] 4.2 Update `reference_map()` to compute stable IDs
    - Call `stable_annotation_id()` for each annotation
    - Populate `stable_id` field in `AnnotationWithId` constructor
    - _Requirements: 5.1, 5.2, 5.3_

  - [x] 4.3 Write property test for cross-run determinism
    - **Property 3: Cross-Run Determinism**
    - **Validates: Requirements 2.5, 5.4, 6.4, 9.1, 9.3**

- [ ] 5. Update JSON report output
  - [ ] 5.1 Modify annotation serialization in `json.rs`
    - Add `stable_id` field to annotation JSON object
    - Place after `source` field for logical grouping
    - _Requirements: 6.1, 6.2, 6.3_

  - [ ]* 5.2 Write property test for JSON output completeness
    - **Property 6: JSON Output Completeness**
    - **Validates: Requirements 6.1, 6.2, 6.3**

- [ ] 6. Checkpoint - Verify JSON output
  - Run `cargo xtask test` to ensure tests pass
  - Manually verify JSON output includes `stable_id` field
  - Ensure all tests pass, ask the user if questions arise

- [ ] 7. Update integration tests
  - [ ] 7.1 Update integration test snapshots
    - Run integration tests to generate new snapshots with `stable_id`
    - Review snapshots to verify `stable_id` appears correctly
    - _Requirements: 8.4, 10.4_

  - [ ]* 7.2 Add integration test for cross-run consistency
    - Generate report twice from same source
    - Verify `stable_id` values are identical
    - _Requirements: 9.1, 10.4_

- [ ] 8. Final checkpoint - Run full test suite
  - Run `cargo xtask test` to verify all tests pass
  - Run `cargo xtask checks` to verify clippy and rustfmt pass
  - Ensure all tests pass, ask the user if questions arise

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- Each task references specific requirements for traceability
- The implementation follows the data flow: hash function → ID generation → struct extension → JSON output
- FNV-1a is implemented inline (no external crate) per the plan document
- Sequential IDs (`id: usize`) are preserved for v1 backward compatibility
- The frontend continues using array indices; `stable_id` is inert metadata until v2
