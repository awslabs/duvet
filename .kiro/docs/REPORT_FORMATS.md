# Duvet Report Formats

Context document describing JSON and snapshot report formats and their relationship to internal data structures.

## Internal Data Model

### Core Types (duvet/src/annotation.rs)

```
Annotation
├── source: Path              # Source file containing the annotation
├── anno_line: usize          # Line number in source
├── anno: AnnotationType      # Type of annotation
├── target: String            # Target spec path (may include #section)
├── quote: String             # Quoted text from spec
├── comment: String           # User comment
├── level: AnnotationLevel    # Requirement level
├── format: Format            # Spec format hint
├── tracking_issue: String    # Issue tracker link
├── feature: String           # Feature flag
└── tags: BTreeSet<String>    # Custom tags
```

```
AnnotationType = Spec | Citation | Implication | Test | Exception | Todo
AnnotationLevel = Auto | May | Should | Must
```

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
- `!LEVEL` - Requirement level prefix (MAY, SHOULD, MUST) if not Auto
- `implementation` - Has citation annotation
- `implication` - Has implication annotation  
- `test` - Has test annotation
- `exception` - Has exception annotation
- `todo` - Has todo annotation

Example: `TEXT[!MUST,implementation,test]: requirement text`

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

Keyed by target path (URL or file path):

```json
{
  "path/to/spec.md": {
    "title": "Spec Title",
    "format": "markdown",
    "requirements": [0, 2, 5],     // Annotation IDs marked as SPEC type
    "sections": [
      {
        "id": "section-id",
        "title": "Section Title",
        "lines": [ ... ],          // Annotated line content
        "requirements": [0]        // SPEC annotations in this section
      }
    ]
  }
}
```

### sections[].lines

Each line is either:
- Plain string (no annotations)
- Array of annotated segments: `[[annotation_ids], ref_status_id, "text"]`

Segment structure:
```json
[
  [0, 1],      // Array of annotation IDs covering this text
  16,          // Index into "refs" array for status lookup
  "text here"  // The actual text content
]
```

### annotations

Array of all annotations, ordered by ID:

```json
{
  "source": "src/lib.rs",
  "target_path": "https://example.com/spec",
  "target_section": "section-1",      // Optional
  "line": 42,                         // Optional, 0 if not set
  "type": "CITATION",                 // Optional, omitted if Citation
  "level": "MUST",                    // Optional, omitted if Auto
  "comment": "...",                   // Optional
  "feature": "...",                   // Optional
  "tracking_issue": "...",            // Optional
  "tags": ["tag1", "tag2"]            // Optional
}
```

### statuses

Per-annotation coverage statistics, keyed by annotation ID:

```json
{
  "0": {
    "spec": 65,           // Total spec text bytes
    "incomplete": 0,      // Uncovered bytes
    "citation": 65,       // Citation coverage bytes
    "implication": 0,
    "test": 65,
    "exception": 0,
    "todo": 0,
    "related": [1, 2]     // Related annotation IDs
  }
}
```

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
```
id = (todo << 0) | (exception << 1) | (test << 2) | 
     (implication << 3) | (citation << 4) | (spec << 5) +
     (level_index * 64)
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

## Data Flow

```
Source Files → Annotation Parsing → AnnotationSet
                                         ↓
Specifications → Section Parsing → SpecificationMap
                                         ↓
                              Reference Matching
                                         ↓
                                   ReportResult
                                    ↓      ↓
                              JSON Report  Snapshot Report
```

---

## Key Implementation Notes

1. **Annotation IDs**: Assigned sequentially when building `AnnotationReferenceMap` from sorted `AnnotationSet`

2. **Reference Status**: Computed per-text-segment by combining all overlapping annotation types

3. **JSON refs table**: Pre-computed lookup to minimize JSON size - segments store index instead of full status object

4. **Snapshot format**: Designed for human review and git diffing - groups by spec/section, shows only annotated text

5. **Status calculation**: Coverage is byte-offset based, not line-based. A line can be partially covered.
