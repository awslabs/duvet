# Rust Development Guidelines

## Cargo.toml
- All Rust crates in this project use `edition = "2024"`. This ensures we have access to the latest Rust features and improvements.
- Put `features` above dependencies. Add a brief comment about what each feature does.
- Dependencies MUST be lexically sorted
- Use the `testing` feature to enable testing utilities for external crates to use in their own tests

## Code Organization
- Each crate should have a clear, single responsibility
- Public APIs should be well-documented with examples
- Use feature flags for optional functionality
- Generated code goes in `src/generated/` directories to ensure tooling has easy access. Anything in these directories MUST NOT be modified directly.
- Don't use `mod.rs`. Instead prefer
  ```
  mymodule.rs
  mymodule/
    mysubmodule.rs
  ```

## Dependencies
- Use workspace-level dependency versions when possible
- Minimize dependencies, especially for core crates
- Consider compile times when adding dependencies
- Use `[workspace.dependencies]` for version management

## Error Handling
- Provide detailed error messages
- Include error context where helpful

## Testing
- All public APIs must have tests
- Use integration tests for complex functionality
- Property-based testing for data structures. Use the `bolero` crate to do this as it supports randomized testing and fuzzing.
- Snapshot testing for generated code. Use the `insta` crate.
- Each unit test should be focused on testing one piece of functionality or requirement. Prefer writing as small of unit tests as possible.

## Documentation
- All public items must be documented
- Include examples in doc comments
- Cite relevant RFCs/design docs. We use `duvet` to ensure any citations are actually contained in the document.
- Document error conditions

## Performance
- Profile before optimizing
- Document performance characteristics
- Use benchmarks for critical paths
- Prefer allocating up-front once. Avoid excessive allocations and cloning.
- Prefer static dispatch over dynamic dispatch.

## Style
- Follow standard Rust naming conventions
- Use `rustfmt` with project settings
- Run `clippy` with project lints
- Keep functions focused and small

## Safety
- Minimize use of unsafe code
- Document all unsafe blocks
- Explain safety invariants
- Add safety tests. If possible, the `unsafe` code should be checked with `miri` and `kani`, through `bolero` harnesses.

## Async
- Avoid calling `tokio` functions directly. Instead, we use a runtime `Env` trait to support multiple environments. This includes the `bach` runtime, which is a discrete event simulation framework for async rust. This allows us to test complex task interactions deterministically in non-real time.
- Document blocking operations
- Backpressure is incredibly important. Queues are everywhere and can build up and cause extra latency.
- Handle cancellation properly

## Generated Code
- Clear banner indicating generated status
- Source file attribution
- No manual edits
- Regenerate via build scripts
