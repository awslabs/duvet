# Duvet Audit Command Design

## Problem Statement

**Core Problem**: Test annotations claim to validate specification requirements, but there's no verification that the test actually executes the implementation code that satisfies those requirements.

**Gap**: We have duvet annotations indicating intent, and coverage data indicating execution, but no bridge between them to verify traceability.

**Example Scenario**: A test contains a duvet annotation claiming to validate "The system MUST authenticate users", but the test never actually calls the authentication implementation code. Current duvet tooling cannot detect this disconnect.

## Key Abstractions

### 1. Annotation Execution
**Definition**: An annotation is "executed" if the first executable line of code after the annotation's boundary was covered during the test run.

**Interface**: 
```
is_annotation_executed(annotation: Annotation, coverage: CoverageData) -> bool
```

**Implementation Strategy**:
- Use duvet's existing parsing to determine annotation boundaries
- Use coverage data to identify which lines are executable vs non-executable
- Find the first executable line after the annotation ends
- Check if that line was covered

### 2. Annotation Correlation  
**Definition**: Two annotations are "correlated" if they reference the same specification target and have matching normalized quote text.

**Interface**:
```
find_correlations(annotations: Set<Annotation>) -> Set<(TestAnnotation, Set<ImplementationAnnotation>)>
```

**Matching Rules**:
- Same specification target (file#section)
- Quote text matches using duvet's existing normalization logic
- Supports partial matching (test quotes broader requirements, implementations quote parts)
- One annotation is TEST type, others are CITATION type

### 3. Coverage Completeness
**Definition**: A test annotation has "complete coverage" if all parts of its quoted requirement are covered by executed implementation annotations.

**Interface**:
```
verify_complete_coverage(test_quote: String, impl_quotes: Set<String>) -> bool
```

**Validation Logic**:
- All semantic content of test quote must be covered by implementation quotes
- Whitespace-only gaps are acceptable (following duvet's normalization)
- Partial coverage is not acceptable (all parts must be covered)

## Core Algorithm

### Per-Test Coverage Algorithm (Optimal)
```
Input: per_test_coverage_data, duvet_annotations
Output: AuditResult

1. results = []
2. for each test in per_test_coverage_data.get_tests():
     // Find annotations executed by this specific test
     executed_annotations = filter(duvet_annotations, is_executed_by_test(test, per_test_coverage_data))
     
     test_annotations = filter(executed_annotations, type == TEST)
     impl_annotations = filter(executed_annotations, type == CITATION)
     
     // Find correlations for this specific test
     correlations = find_correlations(test_annotations, impl_annotations)
     
     for each (test_annotation, matching_impls) in correlations:
       if verify_complete_coverage(test_annotation.quote, matching_impls.quotes):
         results.add(PASS(test, test_annotation))
       else:
         results.add(FAIL(test, test_annotation))
         
3. return aggregate(results)
```

### Generic Coverage Algorithm (Fallback)
```
Input: generic_coverage_data, duvet_annotations
Output: AuditResult

1. executed_annotations = filter(duvet_annotations, is_executed_using(generic_coverage_data))
2. test_annotations = filter(executed_annotations, type == TEST)  
3. impl_annotations = filter(executed_annotations, type == CITATION)
4. correlations = find_correlations(test_annotations, impl_annotations)
5. results = []
6. for each (test_annotation, matching_impls) in correlations:
     if verify_complete_coverage(test_annotation.quote, matching_impls.quotes):
       results.add(PASS)
     else:
       results.add(FAIL)
7. return aggregate(results)
```

### Unified Algorithm
```
Input: coverage_data, duvet_annotations
Output: AuditResult

if coverage_data.granularity() == PER_TEST:
  return per_test_algorithm(coverage_data.as_per_test(), duvet_annotations)
else:
  return generic_algorithm(coverage_data.as_generic(), duvet_annotations)
```

## Interface Requirements

### Coverage Data Interface

**Two Coverage Granularities:**

#### Per-Test Coverage
```
PerTestCoverageData {
  get_tests() -> Set<TestIdentifier>
  get_covered_lines(test: TestIdentifier, file_path: String) -> Set<LineNumber>
  get_executable_lines(file_path: String) -> Set<LineNumber>
}
```

#### Generic Coverage  
```
GenericCoverageData {
  get_covered_lines(file_path: String) -> Set<LineNumber>
  get_executable_lines(file_path: String) -> Set<LineNumber>
}
```

**Unified Interface:**
```
CoverageData {
  granularity() -> CoverageGranularity  // PER_TEST or GENERIC
  as_per_test() -> Option<PerTestCoverageData>
  as_generic() -> GenericCoverageData
}
```

**Supported Formats**:
- **Clover XML**: Per-test coverage (can iterate through individual tests)
- **JaCoCo XML**: Generic coverage (aggregate results only)  
- **LCOV**: Generic coverage (aggregate results only)

### Annotation Interface  
```
Annotation {
  file_path: String
  line_number: LineNumber
  type: AnnotationType  // TEST or CITATION
  target: SpecificationTarget
  quote: String
  boundary_end: LineNumber  // Computed by duvet parsing
}

SpecificationTarget {
  file_path: String
  section_id: String
}
```

### Audit Result Interface
```
AuditResult {
  overall_status: PASS | FAIL
  correlation_results: List<CorrelationResult>
  summary: AuditSummary
}

CorrelationResult {
  test_annotation: Annotation
  matching_implementations: Set<Annotation>
  status: PASS | FAIL
  error_message: Optional<String>
}

AuditSummary {
  total_executed_test_annotations: usize
  successful_correlations: usize
  failed_correlations: usize
}
```

## Success Criteria

### Overall Results
- **PASS**: All executed test annotations have complete implementation coverage
- **FAIL**: Any executed test annotation lacks complete implementation coverage

### Individual Correlation Results
- **PASS**: Test annotation's quote is fully covered by executed implementation annotations
- **FAIL**: Test annotation's quote has gaps not covered by any executed implementation annotation

### Edge Cases
- **Zero executed test annotations**: PASS (vacuous truth - 0/0 = 100%)
- **Ambiguous matches**: PASS with warning (1 test matches multiple implementations, some covered)
- **Missing implementations**: FAIL (test annotation has no matching implementation annotations)

## Pluggable Components

### 1. Coverage Parser
**Interface**: `trait CoverageParser`
```
parse(file_path: Path) -> Result<CoverageData>
```

**Implementations**:
- `JacocoXmlParser`: Parse JaCoCo XML reports
- `LcovParser`: Parse LCOV trace files (future)
- `CloverParser`: Parse Clover XML reports (future)

### 2. Quote Normalizer
**Reuse**: Duvet's existing text normalization logic
- Handles whitespace normalization for flexible matching
- Supports multi-line quotes and different formatting styles

### 3. Annotation Boundary Detector
**Reuse**: Duvet's existing annotation parsing infrastructure
- Determines where annotations end in source code
- Handles different comment styles and annotation formats

## Configuration Integration

### Complete .duvet/config.toml Structure

The audit command integrates with duvet's existing configuration system. Here's a complete example showing how to add audit configuration to your `.duvet/config.toml`:

```toml
# .duvet/config.toml - Complete configuration with audit support
"$schema" = "https://awslabs.github.io/duvet/config/v0.4.0.json"

# Existing duvet configuration sections
[[source]]
pattern = "src/**/*.rs"
comment-style = { meta = "//=", content = "//#" }
type = "implementation"

[[source]]
pattern = "tests/**/*.rs"
comment-style = { meta = "//=", content = "//#" }
type = "test"

[[specification]]
source = "https://example.com/specification.md"
format = "markdown"

[[requirement]]
pattern = ".duvet/requirements/**/*.toml"

[report]
html = { enabled = true, path = ".duvet/reports/report.html" }
json = { enabled = true, path = ".duvet/reports/report.json" }
snapshot = { enabled = false, path = ".duvet/snapshot.txt" }

# NEW: Audit configuration section
[audit]
enabled = true
coverage-report = "target/coverage/jacoco.xml"
coverage-format = "jacoco-xml"
output-format = "text"
verbose = false

# Optional: Multiple coverage configurations for different test environments
[[audit.coverage]]
name = "unit-tests"
report = "target/coverage/unit-tests.xml"
format = "jacoco-xml"

[[audit.coverage]]
name = "integration-tests"
report = "target/coverage/integration-tests.xml"
format = "clover"

[[audit.coverage]]
name = "e2e-tests"
report = "target/coverage/e2e.lcov"
format = "lcov"
```

### Audit Configuration Options

#### Required Fields
None - all audit configuration is optional with sensible defaults.

#### Optional Fields

**`[audit]` Section:**
- `enabled` (boolean, default: `true`): Enable/disable audit functionality
- `coverage-report` (string, optional): Default path to coverage report file
- `coverage-format` (string, optional): Default coverage format
  - Valid values: `"jacoco-xml"`, `"lcov"`, `"clover"`
- `output-format` (string, default: `"text"`): Default output format
  - Valid values: `"text"`, `"json"`, `"both"`
- `verbose` (boolean, default: `false`): Enable verbose output by default

**`[[audit.coverage]]` Array (optional):**
Multiple named coverage configurations for different test suites:
- `name` (string, required): Unique name for this coverage configuration
- `report` (string, required): Path to coverage report file for this configuration
- `format` (string, required): Coverage format for this configuration
  - Valid values: `"jacoco-xml"`, `"lcov"`, `"clover"`

### Configuration Examples

#### Minimal Configuration
```toml
"$schema" = "https://awslabs.github.io/duvet/config/v0.4.0.json"

# Your existing duvet config...

[audit]
coverage-report = "coverage/jacoco.xml"
coverage-format = "jacoco-xml"
```

#### Multi-Environment Configuration
```toml
"$schema" = "https://awslabs.github.io/duvet/config/v0.4.0.json"

# Your existing duvet config...

[audit]
enabled = true
output-format = "both"
verbose = true

[[audit.coverage]]
name = "unit"
report = "target/coverage/unit.xml"
format = "jacoco-xml"

[[audit.coverage]]
name = "integration"
report = "target/coverage/integration.xml"
format = "clover"

[[audit.coverage]]
name = "rust-tests"
report = "target/coverage/rust.lcov"
format = "lcov"
```

#### CI/CD Optimized Configuration
```toml
"$schema" = "https://awslabs.github.io/duvet/config/v0.4.0.json"

# Your existing duvet config...

[audit]
enabled = true
coverage-report = "${{ env.COVERAGE_REPORT || 'target/coverage/default.xml' }}"
coverage-format = "${{ env.COVERAGE_FORMAT || 'jacoco-xml' }}"
output-format = "json"
verbose = false
```

### Configuration Schema

```rust
#[derive(Clone, Debug, Deserialize)]
pub struct AuditConfig {
    #[serde(default = "AuditConfig::default_enabled")]
    pub enabled: bool,
    
    #[serde(rename = "coverage-report")]
    pub coverage_report: Option<String>,
    
    #[serde(rename = "coverage-format")]  
    pub coverage_format: Option<CoverageFormat>,
    
    #[serde(rename = "output-format", default)]
    pub output_format: OutputFormat,
    
    #[serde(default)]
    pub verbose: bool,
    
    #[serde(default, rename = "coverage")]
    pub coverage_configs: Vec<CoverageConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CoverageConfig {
    pub name: String,
    pub report: String,
    pub format: CoverageFormat,
}

#[derive(Clone, Debug, Deserialize)]
pub enum CoverageFormat {
    #[serde(rename = "jacoco-xml")]
    JacocoXml,
    #[serde(rename = "lcov")]
    Lcov, 
    #[serde(rename = "clover")]
    Clover,
}

#[derive(Clone, Debug, Deserialize)]
pub enum OutputFormat {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "json")]
    Json,
    #[serde(rename = "both")]
    Both,
}
```

## CLI Interface

```bash
duvet audit [OPTIONS]
```

**Optional Parameters**:
- `--coverage-report <path>`: Override config's coverage report path
- `--coverage-format <format>`: Override config's coverage format (jacoco-xml, lcov, clover)
- `--coverage-config <name>`: Use specific coverage configuration from config file
- `--verbose`: Enable verbose output (overrides config setting)
- `--json`: Output results in JSON format (overrides config setting)
- `--quiet`: Suppress all output except errors

**Configuration Precedence** (highest to lowest):
1. Command line arguments
2. Configuration file settings
3. Built-in defaults

**Exit Codes**:
- `0`: All correlations successful (PASS)
- `1`: One or more correlations failed (FAIL) or system error
- `2`: Configuration error or missing required parameters

**Examples**:
```bash
# Use config file defaults
duvet audit

# Override coverage report path
duvet audit --coverage-report target/custom-coverage.xml

# Use specific coverage configuration
duvet audit --coverage-config integration-tests

# Enable verbose output with JSON format
duvet audit --verbose --json
```

## Design Principles

1. **Language Agnostic**: Algorithm works regardless of source language
2. **Coverage Format Agnostic**: Abstracted coverage interface supports multiple formats  
3. **Granularity Adaptive**: Works optimally with per-test coverage, gracefully degrades with aggregate coverage
4. **Zero False Positives**: Only pass when we can prove the correlation exists
5. **Reuse Existing Infrastructure**: Leverage duvet's annotation parsing and text processing
6. **Clear Error Reporting**: Provide actionable diagnostics for failed correlations
7. **Extensible**: Pluggable architecture for adding new coverage formats

## Implementation Strategy

### Phase 1: Core Infrastructure
- Implement coverage data abstraction and JaCoCo parser
- Integrate with duvet's annotation parsing system
- Build correlation engine with quote matching

### Phase 2: Audit Logic
- Implement execution detection using coverage boundaries
- Build correlation validation with complete coverage requirements
- Add comprehensive error reporting

### Phase 3: CLI Integration
- Add audit subcommand to duvet CLI
- Implement result formatting and exit code handling
- Add verbose and JSON output options

### Phase 4: Additional Coverage Formats
- Implement LCOV parser for broader language support
- Implement Clover parser for per-test granularity
- Add format auto-detection capabilities

## Future Enhancements

1. **Test Coverage Completeness**: Separate audit to verify all test annotations are executed
2. **Batch Processing**: Support for multiple coverage reports/test runs
3. **Integration Reporting**: Detailed analytics on requirement coverage trends
4. **IDE Integration**: Real-time feedback on annotation correlation status
