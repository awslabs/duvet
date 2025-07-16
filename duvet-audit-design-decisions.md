# Duvet Audit Command: Design Decisions and Trade-offs

## Context

When we set out to create the `duvet audit` command, we had a clear problem: test annotations claim to validate specification requirements, but there's no way to verify that tests actually execute the implementation code they claim to validate. 

This document captures the key design decisions we wrestled with, the alternatives we considered, and why we chose our recommended approach.

## Reference Materials

- **Existing Python Implementation**: `/Users/ryanemer/source/private-aws-encryption-sdk-dafny-staging/AwsLockbox/runtimes/java/duvet_test_verifier.py`
- **Specification Document**: `/Users/ryanemer/source/private-aws-encryption-sdk-dafny-staging/llm_context/duvet_test_verification_spec.md`
- **Technical Design**: `duvet-audit-design.md`

## Key Design Decisions

### 1. Annotation Execution Detection Strategy

**The Challenge**: How do we determine if a duvet annotation was "executed" during a test run?

**Options Considered**:
- **Option A**: Check if the annotation comment line itself appears in coverage data
- **Option B**: Check lines immediately after the annotation using a fixed buffer
- **Option C**: Use coverage data to find the next executable line after annotation boundaries

**Decision**: We chose **Option C** - Use coverage data to find the next executable line after annotation boundaries.

**Reasoning**: 
- Option A doesn't work because comment lines aren't executable and won't appear in coverage
- Option B is fragile because it uses arbitrary line buffers that might miss executable code or include unrelated code
- Option C is the most accurate because it uses the coverage tool's own understanding of what lines are executable, then finds the first such line after the annotation ends

**Implementation**: Parse annotation boundaries using duvet's existing parsing, then use coverage data to identify executable vs non-executable lines, then check if the first executable line after the annotation was covered.

### 2. Test vs Implementation File Classification

**The Challenge**: How do we distinguish between test files and implementation files to separate TEST annotations from CITATION annotations?

**Options Considered**:
- **Option A**: Classify files by path patterns (e.g., `src/test/` vs `src/main/`)
- **Option B**: Classify files by filename patterns (e.g., `*Test.java` vs other files)
- **Option C**: Drop file classification entirely and rely on annotation types

**Decision**: We chose **Option C** - Drop file classification and rely on annotation types.

**Reasoning**:
- Option A works for Java but breaks down for languages like Rust where tests can be in the same files as implementation
- Option B has similar language-specific limitations
- Option C is language-agnostic and leverages duvet's existing annotation type system (TEST vs CITATION)
- This simplifies the algorithm: just filter all executed annotations by type, regardless of which file they're in

**Key Insight**: The user pointed out that in Rust, "test files" vs "implementation files" don't exist as separate concepts - tests and implementations often coexist in the same file.

### 3. Quote Matching and Coverage Completeness

**The Challenge**: How should we handle cases where test annotations quote broader requirements than implementation annotations?

**Options Considered**:
- **Option A**: Require exact quote matching between test and implementation annotations
- **Option B**: Allow partial matching where implementation quotes are subsets of test quotes
- **Option C**: Allow partial matching but require complete coverage (all parts of test quote must be covered)

**Decision**: We chose **Option C** - Allow partial matching but require complete coverage.

**Reasoning**:
- Option A is too restrictive - tests often validate broader requirements than individual implementation pieces
- Option B creates false positives where tests claim to validate requirements they don't fully cover
- Option C provides flexibility for granular implementation tracking while ensuring comprehensive test coverage
- Uses duvet's existing whitespace normalization for robust matching across different formatting styles

**Example**: Test quotes "System MUST: 1. Load config 2. Validate settings" and implementations quote "Load config" and "Validate settings" separately - this passes because all parts are covered.

### 4. Coverage Format Abstraction

**The Challenge**: Different coverage tools provide different levels of granularity - some give per-test coverage, others only aggregate coverage.

**Options Considered**:
- **Option A**: Design only for aggregate coverage formats (JaCoCo, LCOV)
- **Option B**: Design only for per-test coverage formats (Clover)
- **Option C**: Create dual algorithms that work optimally with per-test but gracefully degrade to aggregate

**Decision**: We chose **Option C** - Dual algorithms with graceful degradation.

**Reasoning**:
- Option A misses the precision benefits of per-test coverage when available
- Option B limits adoption to only tools that provide per-test granularity
- Option C maximizes precision when possible while maintaining broad compatibility
- Per-test coverage can prove "test_foo specifically validates implementation X" vs aggregate coverage can only prove "some test validates implementation X"

**Implementation**: Unified interface that detects granularity and routes to the appropriate algorithm.

### 5. Algorithm Efficiency Optimization

**The Challenge**: The initial per-test algorithm had redundant filtering operations.

**Original Approach**:
```
test_executed_annotations = filter(duvet_annotations, is_executed_by_test(test, coverage))
impl_executed_annotations = filter(duvet_annotations, is_executed_by_test(test, coverage))
```

**Optimized Approach**:
```
executed_annotations = filter(duvet_annotations, is_executed_by_test(test, coverage))
test_annotations = filter(executed_annotations, type == TEST)
impl_annotations = filter(executed_annotations, type == CITATION)
```

**Decision**: We chose the optimized approach.

**Reasoning**: The user pointed out that both filters were identical, so we could filter once for execution, then separate by annotation type. This is more efficient and cleaner code.

### 6. Integration with Duvet Configuration System

**The Challenge**: Should the audit command be standalone or integrate with duvet's existing configuration?

**Options Considered**:
- **Option A**: Standalone configuration file for audit settings
- **Option B**: Command-line only configuration (no config file support)
- **Option C**: Integrate with duvet's existing `.duvet/config.toml` system

**Decision**: We chose **Option C** - Integrate with existing duvet configuration.

**Reasoning**:
- Option A creates configuration fragmentation and maintenance burden
- Option B lacks flexibility for different environments and CI/CD workflows
- Option C leverages duvet's existing configuration infrastructure, provides templating support, and maintains consistency with other duvet commands
- Supports multiple coverage configurations for different test environments (unit, integration, e2e)

**Implementation**: Added `[audit]` section and `[[audit.coverage]]` arrays to existing config schema with full backward compatibility.

### 7. CLI Interface Design

**The Challenge**: Balance between simplicity and flexibility in the command-line interface.

**Options Considered**:
- **Option A**: Require all parameters via CLI arguments
- **Option B**: Config-file only with no CLI overrides
- **Option C**: Config-first with CLI overrides and precedence rules

**Decision**: We chose **Option C** - Config-first with CLI overrides.

**Reasoning**:
- Option A creates verbose commands and poor developer experience
- Option B lacks flexibility for local development and debugging
- Option C provides best of both worlds: simple `duvet audit` for normal use, CLI overrides for special cases
- Precedence: CLI args > Config file > Built-in defaults

### 8. Error Handling for Edge Cases

**The Challenge**: How should we handle various edge cases?

**Decisions Made**:
- **Zero executed test annotations**: PASS (vacuous truth - 0/0 = 100% success)
- **Ambiguous matches** (1 test matches 3 implementations, only 1 covered): PASS with warning
- **Missing implementations**: FAIL (test claims something not implemented)

**Reasoning**: These decisions prioritize avoiding false negatives while providing clear warnings about potential gaps in coverage.

## Architecture Principles That Emerged

Through these decisions, several key principles emerged:

1. **Language Agnostic**: Work regardless of source language or project structure
2. **Granularity Adaptive**: Optimize for best available data while maintaining broad compatibility  
3. **Zero False Positives**: Only pass when we can prove the correlation exists
4. **Reuse Existing Infrastructure**: Leverage duvet's annotation parsing and text processing
5. **Configuration Integration**: Work seamlessly with existing duvet workflows

## What We Learned

The most important insight was recognizing that this problem has two distinct aspects:
1. **Annotation Correlation Audit**: Do executed test annotations have matching executed implementation annotations? (What we're building)
2. **Test Annotation Coverage Audit**: Are your test annotations actually being executed? (Future enhancement)

By clearly separating these concerns, we could focus on the core traceability problem without getting distracted by test execution completeness.

## Implementation Phases

Based on these decisions, we outlined a clear implementation strategy:
1. **Phase 1**: Core infrastructure with JaCoCo support
2. **Phase 2**: Audit logic with correlation validation  
3. **Phase 3**: CLI integration with duvet config system
4. **Phase 4**: Additional coverage formats (LCOV, Clover)

This phased approach allows for incremental value delivery while building toward the full vision.
