# AGENTS.md

Guidelines for AI agents working on the Duvet codebase.

## Project Overview

Duvet is a requirements traceability tool that links implementation code to specification documents. It scans source code for special annotation comments and generates compliance reports.

## Repository Structure

```
duvet/              # Main CLI application (Rust)
  src/              # Core implementation
    annotation.rs   # Annotation parsing and types
    comment/        # Comment tokenizer and parser
    config/         # Configuration schema (TOML)
    extract.rs      # Extract requirements from specs
    init.rs         # Project initialization
    report/         # Report generation (HTML, JSON, snapshot)
    specification/  # Spec parsers (IETF RFC, Markdown)
    target.rs       # Target URL/path handling
    text/           # Text search and normalization
  www/              # React frontend for HTML reports
duvet-core/         # Shared async query framework
duvet-macros/       # Procedural macros for query caching
xtask/              # Build and test automation
specs/              # Test specification files
integration/        # Integration test configurations
guide/              # mdBook documentation
config/             # JSON schema for config validation
```

## Build & Test Commands

```bash
cargo xtask build   # Build (includes www frontend)
cargo xtask test    # Run all tests
cargo xtask checks  # Run clippy and rustfmt
```

Requires `git lfs` for test fixtures.

## Key Concepts

- **Annotations**: Special comments in source code linking to spec sections
- **Specifications**: RFC documents or Markdown files containing requirements
- **Reports**: HTML/JSON/snapshot outputs showing compliance status

## Annotation Format

```rust
//= https://example.com/spec#section-1
//= type=implementation
//# Quoted requirement text from specification
```

Types: `implementation`, `test`, `implication`, `exception`, `todo`, `spec`

## Configuration

Config file: `.duvet/config.toml` (schema v0.4.0)

```toml
'$schema' = "https://awslabs.github.io/duvet/config/v0.4.0.json"

[[source]]
pattern = "src/**/*.rs"

[[specification]]
source = "https://www.rfc-editor.org/rfc/rfc9000"

[report.html]
enabled = true
```

## Code Conventions

- Apache-2.0 license header required on all source files
- Use `cargo xtask checks` before committing
- Nightly rustfmt for formatting
- Snapshot tests use `insta` crate
- Async queries use `duvet-core` framework with `#[duvet_core::query]` macro

## Testing

- Unit tests: inline in source files
- Integration tests: `integration/*.toml` configs with `integration/snapshots/`
- Run specific integration test: check `xtask/src/tests.rs`

## CI

GitHub Actions runs on push/PR to main:
- Multi-platform (Ubuntu, macOS)
- Multiple Rust versions (stable, beta, nightly, MSRV 1.88)
- Checks: clippy, rustfmt, cargo-udeps
