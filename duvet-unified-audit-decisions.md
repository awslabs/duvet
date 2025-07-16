# Duvet Unified Audit: Design Decisions and Trade-offs

## Context

When we set out to expand the `duvet audit` command from a single-purpose correlation checker into a unified specification validation platform, we faced numerous design decisions. This document captures the key choices we wrestled with, the alternatives we considered, and why we chose our recommended approach.

## Reference Materials

- **Existing Python Implementation**: `/Users/ryanemer/source/private-aws-encryption-sdk-dafny-staging/AwsLockbox/runtimes/java/duvet_test_verifier.py`
- **Specification Document**: `/Users/ryanemer/source/private-aws-encryption-sdk-dafny-staging/llm_context/duvet_test_verification_spec.md`
- **Original Technical Design**: `duvet-audit-design.md`
- **Original Design Decisions**: `duvet-audit-design-decisions.md`

## Key Design Decisions

### 1. Terminology: "Implementation" vs "Citations"

**The Challenge**: What should we call the check that verifies requirements have code annotations?

**Options Considered**:
- **Option A**: Keep calling it "citations" (matching existing `--require-citations`)
- **Option B**: Call it "implementation" or "implementation annotations"
- **Option C**: Call it "code coverage" 

**Decision**: We chose **Option B** - "implementation" and "implementation annotations".

**Reasoning**: 
- "Citations" is too academic and doesn't clearly communicate what we're checking
- "Implementation annotation coverage" clearly indicates we're checking for annotations that document implementations
- Avoids confusion with "test coverage" which has an established meaning in software development
- The user specifically requested this change during our discussion to make the purpose clearer

### 2. CLI vs Configuration Behavior

**The Challenge**: How should CLI arguments interact with config.toml settings?

**Options Considered**:
- **Option A**: CLI arguments supplement config.toml (merge behavior)
- **Option B**: CLI arguments override specific config settings (partial override)
- **Option C**: Any CLI argument ignores all config.toml settings (complete override)

**Decision**: We chose **Option C** - Complete CLI override.

**Reasoning**:
- Eliminates confusion about which settings are active
- Creates clear separation between "CI mode" (config-driven) and "development mode" (CLI-driven)
- Predictable behavior: CLI users get exactly what they specify, nothing more
- Supports the dual-use case: stable CI enforcement vs flexible development checking

### 3. Coverage Format Behavior Strategy

**The Challenge**: JaCoCo provides aggregate coverage that can create false positives, while per-test formats provide precise correlation data.

**Options Considered**:
- **Option A**: Only support per-test coverage formats for correlation checking
- **Option B**: Treat all coverage formats the same way (ignore the limitations)
- **Option C**: Adapt behavior based on coverage format capabilities

**Decision**: We chose **Option C** - Format-aware behavior.

**Reasoning**:
- **JaCoCo/Aggregate formats**: Default to individual correlation reporting (no section completeness requirements) to prevent false positives where unrelated tests happen to cover required code
- **Per-test formats**: Support full section completeness validation because we can prove specific test-to-implementation relationships
- Users can override with `--section` to get section enforcement even with JaCoCo when they want strict validation
- Maximizes utility while being honest about limitations

### 4. Section Configuration Strategy

**The Challenge**: Should sections be defined globally, per-check, or both?

**Options Considered**:
- **Option A**: Only global sections (same sections for all checks)
- **Option B**: Only per-check sections (maximum flexibility, more configuration)
- **Option C**: Both global and per-check sections (additive)

**Decision**: We chose **Option C** - Additive section configuration.

**Reasoning**:
- **Global sections**: Handle the common case where most checks apply to the same "done" sections
- **Per-check sections**: Support progressive adoption where some sections are ready for implementation checking but not yet ready for test or coverage checking
- **Additive merging**: `global_sections + check_sections (deduplicated with warnings)`
- **Duplicate warnings**: Help users catch unintentional redundancy without breaking the build
- **Maximum flexibility**: Teams can adopt validation incrementally per section and per check type

### 5. Coverage Configuration Location

**The Challenge**: Where should coverage-specific settings (report path, format) be configured?

**Options Considered**:
- **Option A**: Global audit section (`[audit]` with `coverage-report` and `coverage-format`)
- **Option B**: Inside coverage check section (`[audit.coverage]` with `report` and `format`)
- **Option C**: Separate coverage configuration section

**Decision**: We chose **Option B** - Inside coverage check section.

**Reasoning**:
- **Logical grouping**: Coverage settings only matter when coverage checking is enabled
- **Clean separation**: Implementation and test checks don't need coverage configuration
- **Avoids unused config**: No orphaned coverage settings when coverage checking is disabled
- **Clear ownership**: Coverage check owns its configuration requirements

### 6. Check Enable Flags

**The Challenge**: How should users control which checks run in CI mode?

**Options Considered**:
- **Option A**: Implicit enabling (if sections are configured, check is enabled)
- **Option B**: Explicit enable flags for each check
- **Option C**: Single global enable/disable flag

**Decision**: We chose **Option B** - Explicit enable flags per check.

**Reasoning**:
- **Clear intent**: `enabled = true` makes it obvious which checks will run
- **Granular control**: Teams can enable implementation checking before test checking, etc.
- **Configuration safety**: Prevents accidentally running checks that aren't ready
- **Documentation value**: Configuration file clearly shows the validation strategy

### 7. Global Coverage Check Behavior

**The Challenge**: What should `duvet audit --check coverage` (no --section) do with JaCoCo?

**Original Thinking**: Return an error because section-less coverage checking with aggregate formats is prone to false positives.

**User Insight**: "I want to be able to check individual correlations without section completeness requirements."

**Decision**: **Individual correlation reporting mode** for aggregate formats.

**Reasoning**:
- **Debugging value**: Developers can see which specific test/implementation pairs correlate without being blocked by section completeness requirements
- **False positive prevention**: No section-level pass/fail that could be misleading with aggregate coverage
- **Development workflow**: Supports iterative development where you want to check correlation status as you work
- **Still honest about limitations**: Reports individual correlation status, not section-level guarantees

### 8. Consolidation Strategy

**The Challenge**: How should the new audit command relate to existing duvet functionality?

**Options Considered**:
- **Option A**: Keep existing `--require-citations` and `--require-tests` flags, add audit as additional functionality
- **Option B**: Move functionality into audit and deprecate old flags
- **Option C**: Reimplement functionality cleanly in audit and remove old approach

**Decision**: We chose **Option C** - Clean reimplementation in audit.

**Reasoning**:
- **Single source of truth**: One command for all specification validation needs
- **Consistent interface**: Same configuration and CLI patterns for all check types
- **Better workflow**: Supports both development (flexible) and CI (strict) use cases
- **Eliminates confusion**: No overlap between old and new approaches
- **The user specifically stated**: "we are going to delete all that and move it into the audit"

### 9. Error Handling Philosophy

**The Challenge**: How strict should validation be, and how should edge cases be handled?

**Options Considered**:
- **Option A**: Permissive (warn about issues but don't fail)
- **Option B**: Strict (fail on any validation gap)
- **Option C**: Context-dependent strictness

**Decision**: We chose **Option C** - Context-dependent strictness.

**Reasoning**:
- **CI mode (config-driven)**: Strict validation - any enabled check failure breaks the build
- **Development mode (CLI-driven)**: Actionable feedback - show what's missing without being overly punitive
- **Zero false positives principle**: Only pass when we can prove validation is complete
- **Clear error messages**: Provide specific, actionable feedback to guide development workflow

## What We Learned

### The Development vs CI Insight

The most important insight was recognizing that specification validation has two fundamentally different use cases:
1. **Development**: Iterative, targeted, flexible checking during feature implementation
2. **CI**: Comprehensive, strict validation of completed features

This insight drove the dual-interface design and explains why CLI overrides config completely rather than merging.

### The Coverage Format Reality

We learned that coverage format capabilities fundamentally affect what guarantees we can provide:
- **Aggregate formats**: Can detect individual correlations but not section completeness (due to false positive risk)
- **Per-test formats**: Can provide both individual and section-level guarantees

This led to format-aware behavior rather than trying to make all formats behave identically.

### The Progressive Adoption Need

Teams don't implement all validation types simultaneously. The additive section configuration supports workflows like:
1. Start with implementation annotation coverage for documentation
2. Add test annotation coverage for quality assurance  
3. Add test execution correlation for traceability verification
4. Graduate sections from "in development" to "CI enforced"

## Architecture Principles That Emerged

Through these decisions, several key principles emerged:

1. **Dual Interface Model**: Config-driven CI, CLI-driven development
2. **Progressive Adoption**: Enable validation incrementally per section and check type
3. **Format Awareness**: Adapt behavior to coverage data capabilities and limitations
4. **Zero False Positives**: Only pass when validation is provably complete
5. **Clear Separation**: CLI overrides ignore config to prevent confusion
6. **Additive Configuration**: Flexible section management supporting incremental adoption

## Implementation Implications

These decisions led to a specific implementation strategy:

1. **Phase 1**: Core infrastructure (config parsing, CLI overrides, check abstraction)
2. **Phase 2**: Individual check implementations (reusing existing duvet report logic where possible)
3. **Phase 3**: Coverage correlation with format-aware behavior
4. **Phase 4**: Integration and documentation

The design supports starting with basic implementation/test annotation checking and evolving toward full test execution correlation as teams mature their validation practices.
