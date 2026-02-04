# Duvet Report Formats

Context document describing JSON and snapshot report formats and their relationship to internal data structures.

## Internal Data Model

### Core Types (duvet/src/annotation.rs)

```
Annotation
├── source: Path              # Source file containing the annotation
├── anno_line: usize          # Line number in source
├── original_target: Slice    # Original target text before normalization
├── original_text: Slice      # Original annotation text slice
├── original_quote: Slice     # Original quote before normalization
├── anno: AnnotationType      # Type of annotation
├── target: String            # Target spec path (may include #section)
├── quote: String             # Quoted text from spec (normalized)
├── comment: String           # User comment
├── manifest_dir: Path        # Base directory for resolving relative paths
├── level: AnnotationLevel    # Requirement level
├── format: Format            # Spec format hint
├── tracking_issue: String    # Issue tracker link
├── feature: String           # Feature flag
└── tags: BTreeSet<String>    # Custom tags
```

```
AnnotationType = Spec | Test | Citation (default) | Exception | Todo | Implication
AnnotationLevel = Auto (default) | May | Should | Must
```

Note: When parsing annotation types, `implementation` is accepted as an alias for `Citation`.

### Reference (duvet/src/reference.rs)

Links an annotation to matched text in a specification:

```
Reference
├── target: Arc<Target>           # Specification target
├── annotation: AnnotationWithId  # Annotation with assigned ID
└── text: Slice                   # Matched text slice in spec
```

### Specification (duvet/src/specification.rs)

```
Specification
├── title: Option<String>
├── sections: HashMap<String, Section>
└── format: Format (Auto | Ietf | Markdown)

Section
├── id: String
├── title: String
├── full_title: Slice
└── lines: Vec<Line>

Line = Str(Slice) | Break
```

### Report Result (duvet/src/report.rs)

Top-level structure passed to all report generators:

```
ReportResult<'a>
├── targets: BTreeMap<Arc<Target>, TargetReport>
├── annotations: AnnotationSet    # All annotations (BTreeSet<Arc<Annotation>>)
├── blob_link: Option<&'a str>    # Link template for source files
├── issue_link: Option<&'a str>   # Link template for issues
└── download_path: &'a Path

TargetReport
├── references: Vec<Reference>
├── specification: Arc<Specification>
├── require_citations: bool
├── require_tests: bool
└── statuses: StatusMap
```

### Status Tracking (duvet/src/report/status.rs)

Per-annotation coverage statistics:

```
StatusMap = BTreeMap<AnnotationId, Spec>

Spec
├── spec: usize           # Total spec text offsets
├── incomplete: usize     # Uncovered offsets
├── citation: usize       # Citation coverage
├── implication: usize    # Implication coverage
├── test: usize           # Test coverage
├── exception: usize      # Exception coverage
├── todo: usize           # Todo markers
└── related: BTreeSet<AnnotationId>  # Related annotation IDs
```

---

## Snapshot Report Format

File: `duvet/src/report/snapshot.rs`

Human-readable text format for CI diffing. Used when `[report.snapshot]` is configured.

### Structure

```
SPECIFICATION: [Title](path)
  SECTION: [Section Title](#section-id)
    TEXT[status]: Quoted text from specification
```

### Status Format

Comma-separated list of coverage indicators:
- `!LEVEL` - Requirement level (e.g., `!MAY`, `!SHOULD`, `!MUST`) if not Auto - always appears first when present
- `implementation` - Has Citation annotation (note: called "implementation" in snapshot, "citation" in JSON)
- `implication` - Has Implication annotation  
- `test` - Has Test annotation
- `exception` - Has Exception annotation
- `todo` - Has Todo annotation

Example: `TEXT[!MUST,implementation,test]: requirement text`

Note: The level always appears first (with `!` prefix), followed by other flags in the order listed above.

### CI Mode

When `CI=true` or `--ci` flag:
- Reads existing snapshot file
- Generates new snapshot in memory
- Compares and fails with diff if mismatch

---

## JSON Report Format

File: `duvet/src/report/json.rs`

Machine-readable format for tooling integration.

### Top-Level Structure

```json
{
  "blob_link": "https://...",      // Optional: source file URL template
  "issue_link": "https://...",     // Optional: issue URL template
  "specifications": { ... },       // Spec content keyed by path
  "annotations": [ ... ],          // All annotations
  "statuses": { ... },             // Per-annotation coverage stats
  "refs": [ ... ]                  // Reference status lookup table
}
```

### specifications

**Keyed by the target specification document** (RFC URL or spec file path), NOT by annotation source files.

```json
{
  "https://www.rfc-editor.org/rfc/rfc9000": { ... },
  "../specifications/spec.md": { ... }
}
```

These keys correspond to the `target` field from annotations - the specification being referenced.

Structure per specification:

```json
{
  "path/to/spec.md": {
    "title": "Spec Title",
    "format": "markdown",
    "requirements": [0, 2, 5],
    "sections": [
      {
        "id": "section-id",
        "title": "Section Title",
        "lines": [ ... ],
        "requirements": [0]
      }
    ]
  }
}
```

**requirements field**: Array of annotation IDs where `AnnotationType::Spec`. These are annotations that *define* requirements extracted from the specification (via `duvet extract` or `[[specification]]` config). They mark "this spec text is a requirement needing coverage."

- Spec-level `requirements`: All SPEC-type annotation IDs targeting this specification
- Section-level `requirements`: Only SPEC-type annotation IDs within that section

This is distinct from `Citation`/`Test`/etc. which are annotations in *implementation code* that provide coverage for requirements.

### sections[].lines

Contains the actual specification text with coverage information. Each line is either:
- Plain string (no annotations covering this line)
- Array of annotated segments showing which annotations cover each piece of text

Segment structure:
```json
[
  [0, 1],      // Array of annotation IDs covering this text segment
  16,          // Index into "refs" array for quick coverage status lookup
  "text here"  // The actual specification text
]
```

- **annotation_ids**: Which annotations (from the `annotations` array, by index) cover this text
- **ref_status_id**: Pre-computed coverage status - look up in `refs` array to see coverage flags
- **text**: The specification text content

This allows the report consumer to see:
1. What the specification says (the text)
2. Which annotations reference this text (annotation IDs)
3. Coverage completeness at a glance (ref_status_id → refs lookup)

### annotations

Array of all annotations from both source types, ordered by ID (array index = annotation ID):

**Annotation sources:**
- `SourceFile::Text` - parses `//=` comments from code files (`.rs`, `.c`, `.java`, etc.)
- `SourceFile::Toml` - parses `.duvet/requirements/**/*.toml` files

TOML files always produce `AnnotationType::Spec`. Text files default to `Citation` but can specify any type via `//= type=...`.

```json
{
  "source": "src/lib.rs",                 // Where annotation is defined
  "target_path": "https://example.com/spec",  // Specification being referenced
  "target_section": "section-1",      // Optional
  "line": 42,                         // Optional, 0 if not set
  "type": "CITATION",                 // Optional, omitted if Citation (default)
  "level": "MUST",                    // Optional, omitted if Auto
  "comment": "...",                   // Optional
  "feature": "...",                   // Optional
  "tracking_issue": "...",            // Optional
  "tags": ["tag1", "tag2"]            // Optional
}
```

**Key distinction:**
- `source`: Where the annotation lives (code file or TOML file)
- `target_path`: The specification document being referenced

### statuses

Per-SPEC-annotation coverage statistics, keyed by annotation ID. Only SPEC-type annotations have entries here.

```json
{
  "0": {
    "spec": 65,
    "incomplete": 0,
    "citation": 65,
    "implication": 0,
    "test": 65,
    "exception": 0,
    "todo": 0,
    "related": [1, 2]
  }
}
```

**How coverage works:**

Each annotation has a `quote` field containing text from the specification. During reference matching, this quote is located in the spec file, producing a byte range (start..end offsets from the beginning of the spec file).

A byte offset is "covered by Citation" when a Citation annotation's quote spans that position in the spec. Multiple annotations can cover the same offsets.

**Values are counts of unique byte offsets** in the specification file:

- `spec`: Total byte offsets spanned by this SPEC annotation's quote
- `incomplete`: Byte offsets not yet fully covered (after applying coverage rules below)
- `citation`: Byte offsets where Citation annotations' quotes overlap
- `implication`: Byte offsets where Implication annotations' quotes overlap
- `test`: Byte offsets where Test annotations' quotes overlap
- `exception`: Byte offsets where Exception annotations' quotes overlap
- `todo`: Byte offsets where Todo annotations' quotes overlap
- `related`: IDs of non-SPEC annotations whose quotes overlap with this requirement

**Coverage calculation** (from `status.rs`):
1. Start with all SPEC byte offsets as "incomplete"
2. Remove offsets covered by Exception (fully covered)
3. Remove offsets covered by Implication (fully covered)
4. Remove offsets covered by Citation OR Test (either satisfies coverage)
5. Remaining offsets = `incomplete`

**Note:** The code comment says "an offset needs to be both cited and tested to be complete" but the implementation uses `.union()` which removes offsets covered by **either** type. This is a code comment/implementation mismatch - the actual behavior is that Citation OR Test coverage is sufficient.

**Note on usage**: The byte offset counts appear to be over-engineered for current use. The HTML frontend only uses these values for equality comparisons (e.g., `spec === citation`) to determine complete/incomplete status - the actual numeric values are never displayed. A simpler boolean model would suffice for current functionality. The granular counts could support partial coverage percentages (e.g., "70% covered") but this is not currently implemented.

### refs

Lookup table for reference status combinations. Index corresponds to `ref_status_id` in line segments.

Generated by iterating all combinations of:
- `level`: Auto, May, Should, Must
- `spec`, `citation`, `implication`, `test`, `exception`, `todo`: true/false

```json
[
  {},                                    // Index 0: no coverage
  { "todo": true },                      // Index 1
  { "exception": true },                 // Index 2
  { "exception": true, "todo": true },   // Index 3
  // ... 256 total combinations (4 levels × 2^6 flags)
]
```

ID calculation (json.rs `RefStatus::id()`):
```rust
// Bit positions (LSB to MSB): todo, exception, test, implication, citation, spec
// Then level_index * 2^6 (64) is added
id = (todo << 0) | (exception << 1) | (test << 2) | 
     (implication << 3) | (citation << 4) | (spec << 5) +
     (level_index * 64)
// where level_index: Auto=0, May=1, Should=2, Must=3
```

---

## CI Enforcement (Legacy)

File: `duvet/src/report/ci.rs`

Line-based coverage validation (used when snapshot not configured):

1. Collects all referenced lines per specification
2. Validates:
   - All significant lines have citations (if `require_citations`)
   - All cited lines have tests (if `require_tests`)
   - Exceptions and implications count as fully covered

---

## LCOV Report Format

File: `duvet/src/report/lcov.rs`

Generates LCOV-format files for integration with code coverage tools (e.g., IDE coverage visualization, CI coverage reports).

### Output

One `.lcov` file per specification in the output directory:
```
<output_dir>/compliance.0.lcov
<output_dir>/compliance.1.lcov
...
```

### LCOV Record Structure

```
TN:Compliance
SF:<spec_file_path>
FN:<line>,<section_title>       # Function (section) definitions
FNF:<section_count>             # Total function count
FNDA:<hit_count>,<section>      # Function hit data
BRDA:<line>,<block>,<count>     # Branch data (coverage)
DA:<line>,<hit_count>           # Line hit data
end_of_record
```

### Coverage Model

LCOV uses two "blocks" to track coverage:
- `IMPL_BLOCK` (0,0): Citation/implementation coverage
- `TEST_BLOCK` (1,0): Test coverage

### Annotation Type Mapping

| AnnotationType | Citation Block | Test Block |
|----------------|----------------|------------|
| Citation       | 1              | 0          |
| Test           | 0              | 1          |
| Implication    | 1              | 1          |
| Exception      | 1              | 1          |
| Spec           | 0              | 0          |
| Todo           | 0              | 0          |

### Line Coverage Calculation

Final line coverage (`DA` records) depends on `require_citations` and `require_tests` settings:

| require_citations | require_tests | Line is covered when...              |
|-------------------|---------------|--------------------------------------|
| true              | true          | Both cited AND tested                |
| true              | false         | Cited (tests ignored)                |
| false             | true          | Tested (citations ignored)           |
| false             | false         | Either cited OR tested               |

Note: In the `(false, false)` case, only covered lines emit `DA:line,1` records. Lines not covered by either citation or test do not get explicit `DA:line,0` records (unlike other cases which explicitly mark partial coverage as uncovered).

### Use Case

The LCOV format allows specification coverage to be visualized in tools that understand code coverage, treating specification lines like source code lines. This enables:
- Coverage visualization in IDEs
- Integration with CI coverage reporting tools
- Tracking coverage trends over time

---

## Data Flow

```
Annotation Sources                        Specification Documents
─────────────────                         ───────────────────────
src/**/*.rs (//= comments)                RFC URLs (https://...)
.duvet/requirements/**/*.toml             Local spec files (*.md)
        │                                         │
        ▼                                         ▼
   AnnotationSet                            SpecificationMap
   (all annotations                         (parsed spec content
    with assigned IDs)                       by target path)
        │                                         │
        └──────────────┬──────────────────────────┘
                       ▼
              Reference Matching
              (match annotation.quote to spec text)
                       │
                       ▼
                 ReportResult
                 ├── targets: spec path → TargetReport
                 ├── annotations: all annotations
                 └── statuses: coverage stats per SPEC annotation
                       │
           ┌───────────┴───────────┐
           ▼                       ▼
      JSON Report             Snapshot Report
      (machine-readable)      (human-readable, CI diffing)
```

**Key relationship:** Annotations *reference* specifications. The `specifications` object in JSON is organized by what's being referenced (the spec), showing which annotations provide coverage for each piece of spec text.

---

## Key Implementation Notes

1. **Annotation IDs**: Assigned sequentially when building `AnnotationReferenceMap` from sorted `AnnotationSet`

2. **Reference Status**: Computed per-text-segment by combining all overlapping annotation types

3. **JSON refs table**: Pre-computed lookup to minimize JSON size - segments store index instead of full status object

4. **Snapshot format**: Designed for human review and git diffing - groups by spec/section, shows only annotated text

5. **Status calculation**: Coverage is byte-offset based, not line-based. A line can be partially covered.
