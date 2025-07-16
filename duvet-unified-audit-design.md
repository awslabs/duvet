# Duvet Unified Audit Design: Complete Specification Validation

## Problem Statement

**Core Challenge**: Specification-driven development requires multiple types of validation throughout the development lifecycle, but these validations are currently fragmented and don't support the dual needs of iterative development and CI enforcement.

**Specific Problems**:
1. **Implementation Annotation Gap**: Requirements exist in specifications but lack implementation annotations in code
2. **Test Annotation Gap**: Requirements have implementations but lack corresponding test annotations  
3. **Execution Gap**: Test annotations claim to validate requirements but don't actually execute the implementation code
4. **Workflow Gap**: No unified command supports both flexible development checking and strict CI enforcement

## The Development vs CI Divide

### **Development Use Case: Feature Implementation**
- **Context**: Developer implementing new specification requirements
- **Goal**: Iterative validation of specific aspects during development
- **Needs**: 
  - Check individual validation types (implementation annotations, test annotations, coverage correlation)
  - Target specific specification sections being worked on
  - Non-breaking feedback that guides next steps
- **Example Workflow**: 
  ```bash
  duvet audit --check implementation --section crypto.md#aes-encryption  # Do I have impl annotations?
  duvet audit --check tests --section crypto.md#aes-encryption          # Do I have test annotations?  
  duvet audit --check coverage --section crypto.md#aes-encryption       # Do tests execute implementations?
  ```

### **CI Use Case: Project Integrity**
- **Context**: Continuous integration protecting completed features
- **Goal**: Comprehensive validation that "done" sections remain fully validated
- **Needs**:
  - All validation types must pass for configured sections
  - Configuration-driven (not command-line driven) 
  - Binary pass/fail that can break builds
- **Example Workflow**:
  ```bash
  duvet audit  # Check all complete-sections with all enabled validations
  ```

## Three Validation Dimensions

### **1. Implementation Annotation Coverage (`--check implementation`)**

**Purpose**: Verify that specification requirements have corresponding implementation annotations in code.

**What it checks**: For each requirement in target sections, ensure there exists at least one CITATION annotation that quotes that requirement.

**Failure cases**:
- Requirement text exists in specification but no code annotations reference it
- Implementation annotations reference non-existent specification text
- Implementation annotations exist but are marked as TODO/incomplete

**Example**:
```
Specification: "The system MUST use AES-256 encryption"
Missing: No code contains //= cite-spec: crypto.md#encryption with matching quote
```

**Reporting**:
```
Implementation Annotation Coverage Results:
✗ crypto.md#aes-encryption: 2/3 requirements have implementation annotations
  - Missing: Line 45-67 "Key derivation MUST use PBKDF2"
✓ auth.md#user-authentication: 3/3 requirements have implementation annotations
```

### **2. Test Annotation Coverage (`--check tests`)**

**Purpose**: Verify that specification requirements have corresponding test annotations.

**What it checks**: For each requirement in target sections, ensure there exists at least one TEST annotation that quotes that requirement.

**Failure cases**:
- Requirement has implementation annotations but no test annotations
- Requirement has neither implementation nor test annotations
- Test annotations reference non-existent specification text

**Example**:
```
Specification: "The system MUST use AES-256 encryption"  
Implementation: ✓ Has CITATION annotation in src/crypto.rs
Missing: No test contains //= cite-spec: crypto.md#encryption with TEST annotation
```

**Reporting**:
```
Test Annotation Coverage Results:
✗ crypto.md#aes-encryption: 1/3 requirements have test annotations  
  - Missing tests: Line 23-34 "Encryption keys MUST be rotated"
  - Missing tests: Line 45-67 "Key derivation MUST use PBKDF2"
✓ auth.md#user-authentication: 3/3 requirements have test annotations
```

### **3. Test Execution Correlation (`--check coverage`)**

**Purpose**: Verify that test annotations actually execute the implementation code they claim to validate.

**What it checks**: For each test annotation, ensure it correlates with executed implementation annotations that quote the same specification requirements.

**Failure cases**:
- Test annotation not executed (test didn't run or annotation outside execution path)
- Implementation annotation not executed (implementation code not covered)
- Test and implementation annotations both executed but quote different requirements (no correlation)

**Example**:
```
Test annotation: //= cite-spec: crypto.md#encryption (executed ✓)
Implementation annotation: //= cite-spec: crypto.md#encryption (not executed ✗)
Result: No correlation - test doesn't actually validate the implementation
```

**Reporting**:
```
Test Execution Correlation Results:
✓ crypto.md#aes-encryption: 2/2 correlations successful
✗ auth.md#user-authentication: 1/3 correlations successful
  - TEST annotation at tests/auth_test.rs:45 → No matching implementation coverage
  - Implementation annotation at src/auth.rs:123 → No test coverage
```

## Unified Command Interface

### **Config-Driven Execution (CI Mode)**
```bash
duvet audit
```
- Uses `.duvet/config.toml` to determine which checks to run and which sections to validate
- Runs all enabled checks on their configured sections  
- Binary pass/fail - any failure breaks the build
- Designed for CI pipelines and project integrity

### **CLI-Driven Execution (Development Mode)**
```bash
# Single check, single section
duvet audit --check implementation --section crypto.md#aes-encryption

# Multiple checks, single section  
duvet audit --check implementation,tests --section crypto.md#aes-encryption

# Single check, multiple sections
duvet audit --check tests --section crypto.md#aes-encryption,auth.md#login

# Single check, global scope (format-dependent behavior)
duvet audit --check implementation  # All specifications
duvet audit --check tests           # All specifications  
duvet audit --check coverage        # Format-dependent (see below)
```

**CLI Override Rule**: Any CLI flag ignores all config.toml settings and operates based solely on CLI arguments.

## Configuration Design

### **Basic Configuration Structure**
```toml
[audit]
# Global sections - apply to ALL enabled checks
sections = [
  "crypto.md#aes-encryption",
  "auth.md#user-authentication"
]

[audit.implementation]
enabled = true

[audit.tests]
enabled = true

[audit.coverage]
enabled = true
report = "target/coverage/jacoco.xml"
format = "jacoco-xml"
```

### **Additive Section Configuration**
```toml
[audit]
# Global sections - apply to ALL enabled checks
sections = [
  "crypto.md#aes-encryption",
  "auth.md#user-authentication"
]

[audit.implementation]
enabled = true
# Additional sections only for implementation check
sections = [
  "crypto.md#key-generation",      # implementation-specific
  "auth.md#user-authentication"    # DUPLICATE - will warn but merge
]

[audit.tests]
enabled = true
# Only uses global sections

[audit.coverage]
enabled = true
report = "target/coverage/jacoco.xml"
format = "jacoco-xml"
sections = [
  "crypto.md#signature-validation"  # coverage-specific
]
```

### **Section Resolution Logic**
For each enabled check: `final_sections = global_sections + check_sections (deduplicated with warnings)`

**Example Resolution**:
- **Implementation check gets**: `crypto.md#aes-encryption`, `auth.md#user-authentication`, `crypto.md#key-generation`
- **Tests check gets**: `crypto.md#aes-encryption`, `auth.md#user-authentication`  
- **Coverage check gets**: `crypto.md#aes-encryption`, `auth.md#user-authentication`, `crypto.md#signature-validation`

**Warning for duplicates**: `Warning: Section 'auth.md#user-authentication' defined in both global and implementation check sections`

### **Configuration Examples**

**Minimal CI Configuration**:
```toml
[audit]
sections = ["crypto.md#aes-encryption"]

[audit.implementation]
enabled = true

[audit.tests] 
enabled = true

[audit.coverage]
enabled = true
report = "target/coverage/jacoco.xml"
format = "jacoco-xml"
```

**Progressive Adoption**:
```toml
[audit]
sections = [
  "crypto.md#aes-encryption",      # Fully validated
  "auth.md#user-authentication"   # Fully validated
]

[audit.implementation]
enabled = true
sections = [
  "storage.md#data-persistence",  # Implementation ready, tests/coverage pending
  "network.md#tls-config"         # Implementation ready, tests/coverage pending
]

[audit.tests]
enabled = true
sections = [
  "storage.md#data-persistence"   # Tests ready, coverage pending
]

[audit.coverage]
enabled = true
report = "target/coverage/jacoco.xml"
format = "jacoco-xml"
# Only fully validated sections get coverage checks
```

## Format-Aware Coverage Behavior

### **Aggregate Formats (JaCoCo, LCOV)**

**Individual Correlation Mode** (Default):
```bash
duvet audit --check coverage  # No --section specified
```
- Reports individual correlation status for all annotations
- No section completeness requirements
- Prevents false positives from aggregate coverage data
- Useful for debugging specific correlation issues

**Section Enforcement Mode**:
```bash
duvet audit --check coverage --section crypto.md#aes-encryption  
```
- Requires ALL annotations in the section to be executed AND correlated
- Section fails if any annotation lacks coverage or correlation
- Provides section-level guarantee

### **Per-Test Formats (Clover)**
```bash
duvet audit --check coverage  # Global scope allowed
```
- Can enforce section completeness globally 
- Per-test data eliminates false positive risk from aggregate coverage
- Provides precise "test X validates implementation Y" guarantees

### **Coverage Configuration**
```toml
[audit.coverage]
enabled = true
report = "target/coverage/jacoco.xml"
format = "jacoco-xml"  # Options: "jacoco-xml", "lcov", "clover"
```

**CLI Coverage Overrides**:
```bash
duvet audit --check coverage --coverage-report my-coverage.xml
duvet audit --check coverage --coverage-format clover
duvet audit --check coverage --coverage-report clover.xml --coverage-format clover
```

**Precedence**: CLI flags > Config file > Error (no defaults)

## Success Criteria & Validation Levels

### **Individual Check Success**
- **Implementation Annotation Coverage**: All requirements in target sections have CITATION annotations
- **Test Annotation Coverage**: All requirements in target sections have TEST annotations  
- **Test Execution Correlation**: All executed annotations have proper correlations (format-dependent scope)

### **Section Success** 
- **All enabled checks pass** for all requirements in the section
- **Complete validation chain**: Requirement → Implementation Annotation → Test Annotation → Execution Correlation

### **Audit Success**
- **All configured sections pass** their enabled validation checks
- **Zero false positives**: Only pass when validation is provably complete

### **Edge Cases**
- **Zero configured sections**: PASS (vacuous truth)
- **Disabled checks**: Ignored completely
- **Missing coverage data**: FAIL with clear error message
- **Duplicate section warnings**: WARN but continue processing

## CLI Reference

### **Basic Usage**
```bash
# Config-driven (CI)
duvet audit

# Development checking
duvet audit --check implementation --section crypto.md#aes-encryption
duvet audit --check tests --section auth.md#login,auth.md#logout
duvet audit --check coverage --section crypto.md#aes-encryption

# Multiple checks
duvet audit --check implementation,tests --section crypto.md#aes-encryption

# Global checking (format-dependent for coverage)
duvet audit --check implementation  # All specifications
duvet audit --check tests           # All specifications
duvet audit --check coverage        # Individual correlations (JaCoCo) or global (Clover)
```

### **Coverage Overrides**
```bash
duvet audit --check coverage --coverage-report target/custom.xml
duvet audit --check coverage --coverage-format clover
duvet audit --check coverage --coverage-report clover.xml --coverage-format clover
```

### **Exit Codes**
- `0`: All checks passed
- `1`: One or more checks failed or system error
- `2`: Configuration error or invalid arguments

## Implementation Strategy

### **Phase 1: Core Infrastructure**
- Implement configuration parsing with additive section resolution
- Build check abstraction with enable/disable flags
- Create CLI argument parsing with config override logic

### **Phase 2: Individual Check Implementation**
- Implement Implementation Annotation Coverage check (reuse duvet report logic)
- Implement Test Annotation Coverage check (reuse duvet report logic)
- Implement basic Test Execution Correlation with JaCoCo support

### **Phase 3: Format Support & Polish**
- Add LCOV and Clover coverage format support
- Implement format-aware coverage behavior
- Add comprehensive error reporting and warnings

### **Phase 4: Integration & Documentation**
- Integrate with duvet CLI
- Add configuration schema validation
- Create user documentation and examples

## Design Principles

1. **Dual Interface Model**: Config-driven CI, CLI-driven development
2. **Progressive Adoption**: Enable validation incrementally per section and check type
3. **Format Awareness**: Adapt behavior to coverage data capabilities
4. **Zero False Positives**: Only pass when validation is provably complete
5. **Clear Separation**: CLI overrides ignore config to prevent confusion
6. **Additive Configuration**: Flexible section management with global and check-specific scoping
7. **Actionable Feedback**: Provide specific, actionable error messages for development workflow

This unified audit design transforms duvet from a fragmented set of validation commands into a cohesive specification validation platform that serves both development and CI needs effectively.
