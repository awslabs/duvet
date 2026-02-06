# Multi-Package Report Merge Plan

## Problem Statement

A single specification is implemented across multiple packages. Due to build system constraints, duvet runs in isolation per package. The goal is a unified "single pane of glass" report tracking conformance across all packages.

## Current Limitations

1. ~~**Single blob_link**: `HtmlReport.blob_link` is one string. Merged reports need per-package links since source files live in different repos/paths.~~ ✅ **RESOLVED** - Per-source `blob-link` now supported in `[[source]]` config blocks, propagated to annotations.

2. **JSON format is write-only**: `json.rs` uses streaming macros with no deserialization. Format is optimized for React frontend, not roundtripping.

3. **Ephemeral annotation IDs**: IDs assigned sequentially during `reference_map()`. Merging causes collisions.

4. **No package context**: Annotations track `source` (file path) but not which package. Can't distinguish `src/lib.rs` from package A vs B after merge.

---

## Phase 1: Data Model Enhancements

### 1.1 Support blob links via source config ✅ COMPLETED (commit 75acb7c4)

Extend `[[source]]` blocks to accept `blob-link`:

```toml
[[source]]
pattern = "src/**/*.rs"
blob-link = "https://github.com/org/my-package/blob/main"  # optional, overrides report.html.blob-link
```

Config schema changes (`v0_4_0.rs`):
```rust
pub struct Source {
    pub pattern: String,
    pub blob_link: Option<String>,    // NEW
    // existing fields...
}
```

Internal `Source` struct (`config.rs`):
```rust
pub struct Source {
    pub pattern: String,
    pub root: Path,
    pub comment_style: crate::comment::Pattern,
    pub default_type: crate::annotation::AnnotationType,
    pub blob_link: Option<Arc<str>>,  // NEW
}
```

Resolution logic:
1. Annotation inherits `blob_link` from its source config
2. If source has no `blob-link`, fall back to `report.html.blob-link`

This keeps per-package config self-contained in each package's `.duvet/config.toml`.

**Implementation notes:**
- `blob_link` field added to `Annotation` struct (`annotation.rs`)
- Comment parser propagates `blob_link` from source config to parsed annotations
- JSON report includes `blob_link` per annotation when present
- Frontend (`www/src/result.js`) uses annotation's `blob_link` if present, falls back to global

### 1.2 Stable annotation identifiers

Replace sequential `usize` IDs with content-based composite key:
```
(source_path, target_path, target_section, quote_hash) → deterministic ID
```

Same annotation across runs gets same ID.

---

## Phase 2: Roundtrip-Friendly JSON Format

### 2.1 Define v2 JSON schema

Use serde derive instead of streaming macros:

```rust
#[derive(Serialize, Deserialize)]
pub struct ReportV2 {
    pub version: String,  // "2.0"
    pub blob_link: Option<String>,
    pub issue_link: Option<String>,
    pub specifications: BTreeMap<String, SpecificationV2>,
    pub annotations: Vec<AnnotationV2>,
    pub coverage: BTreeMap<String, CoverageStatus>,
}
```

**specifications key**: The target path - either a URL (`https://www.rfc-editor.org/rfc/rfc9000`) or local file path (`../specs/my-spec.md`).

```rust
#[derive(Serialize, Deserialize)]
pub struct SpecificationV2 {
    pub title: Option<String>,
    pub format: String,  // "ietf" or "markdown"
    pub sections: Vec<SectionV2>,
}

#[derive(Serialize, Deserialize)]
pub struct SectionV2 {
    pub id: String,           // e.g., "section-2.1"
    pub title: String,        // e.g., "Request Methods"
    pub lines: Vec<LineV2>,   // pre-segmented lines with coverage
    pub requirements: Vec<String>,  // stable IDs of SPEC annotations in this section
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum LineV2 {
    Plain(String),                    // no annotations on this line
    Segmented(Vec<LineSegmentV2>),    // line split by annotation coverage
}

#[derive(Serialize, Deserialize)]
pub struct LineSegmentV2 {
    pub annotation_ids: Vec<String>,  // stable IDs of annotations covering this segment
    pub status_id: usize,             // index into refs table
    pub text: String,                 // the actual text
}
```

**What is segmentation?**

A spec line may have multiple annotations covering different (possibly overlapping) byte ranges. Segmentation splits the line at annotation boundaries so each piece knows which annotations cover it.

Example line: `A server MUST accept both GET and POST requests.`

With annotations:
- Annotation A (SPEC): covers entire line
- Annotation B (Citation): covers "MUST accept both GET"
- Annotation C (Test): covers "accept both GET and POST"

**Plain** (no annotations): `"A server MUST accept both GET and POST requests."`

**Segmented** (split at boundaries):
```json
[
  { "annotation_ids": ["A"],           "status_id": 32, "text": "A server " },
  { "annotation_ids": ["A", "B"],      "status_id": 48, "text": "MUST " },
  { "annotation_ids": ["A", "B", "C"], "status_id": 52, "text": "accept both GET" },
  { "annotation_ids": ["A", "C"],      "status_id": 36, "text": " and POST" },
  { "annotation_ids": ["A"],           "status_id": 32, "text": " requests." }
]
```

The frontend renders each segment with different styling (underline color = coverage status) and shows applicable annotations on click.

Byte offsets are used during segmentation to compute split points, then discarded. Only the final text segments with annotation IDs are serialized.

**Open design question: refs table vs inline status vs bitmap**

Option A - refs table (matches v1):
```json
{
  "refs": [ {}, { "spec": true }, { "citation": true }, ... ],
  "lines": [[{ "annotation_ids": ["A"], "status_id": 32, "text": "..." }]]
}
```

Option B - inline status:
```json
{
  "lines": [[{ "annotation_ids": ["A"], "status": { "spec": true, "level": "MUST" }, "text": "..." }]]
}
```

Option C - bitmap:
```json
{
  "lines": [[{ "annotation_ids": ["A"], "status": 48, "text": "..." }]]
}
```

Bitmap encoding (8 bits total):
- bits 0-5: `spec`, `citation`, `implication`, `test`, `exception`, `todo`
- bits 6-7: `level` (0=AUTO, 1=MAY, 2=SHOULD, 3=MUST)

| Approach | Pros | Cons |
|----------|------|------|
| `refs` table + `status_id` | Smaller JSON, matches v1 | Extra indirection, must include full 256-entry table |
| Inline `status` | Self-contained segments, simpler merge | Repeated objects, larger file |
| Bitmap | Smallest JSON, no refs table needed, fast bitwise merge (`a | b`) | Less readable, fixed capacity (would need `u16` if more types added) |

Decision deferred - all work for the merge use case.

**Open design question: coverage location**

Option A - top-level `coverage` map (current plan):
```json
{
  "specifications": { ... },
  "annotations": [ ... ],
  "coverage": {
    "A": { "spec": 59, "incomplete": 25, "citation": 34, "related": ["B"] }
  }
}
```

Option B - inline coverage into specification sections:
```json
{
  "specifications": {
    "https://example.com/rfc": {
      "sections": [{
        "id": "section-2.1",
        "lines": [ ... ],
        "requirements": ["A"],
        "coverage": {
          "A": { "spec": 59, "incomplete": 25, "citation": 34, "related": ["B"] }
        }
      }]
    }
  }
}
```

| Approach | Pros | Cons |
|----------|------|------|
| Top-level `coverage` | Flat structure, easy to iterate all coverage | Must cross-reference to find which section |
| Inline in section | Coverage co-located with spec content | Nested structure, coverage split across sections |

For merge: top-level is simpler (union two maps). For rendering: inline avoids lookups. Decision deferred.

Lines are **pre-segmented** with coverage data baked in. This means:
- No byte offsets stored (they're an implementation detail of segmentation)
- Frontend can render directly from v2 without re-parsing specs
- Merge combines segments from multiple packages

The segmentation is computed once during `duvet report` using the existing byte-offset logic in `json.rs`, then serialized as final `[ids, status, text]` tuples.

```rust
#[derive(Serialize, Deserialize)]
pub struct AnnotationV2 {
    pub id: String,  // stable hash-based ID
    pub source: String,
    pub blob_link: Option<String>,  // resolved link for this annotation's source
    pub target_path: String,
    pub target_section: Option<String>,
    pub quote: String,
    pub anno_type: AnnotationType,
    pub level: AnnotationLevel,
    pub line: Option<usize>,
    pub comment: Option<String>,
    pub feature: Option<String>,
    pub tracking_issue: Option<String>,
    pub tags: Vec<String>,
}
```

**coverage key**: The stable annotation ID (same as `AnnotationV2.id`) for SPEC-type annotations only. These represent requirements that need coverage.

```rust
#[derive(Serialize, Deserialize)]
pub struct CoverageStatus {
    // Byte offset counts (matching current Spec struct in status.rs)
    pub spec: usize,        // total byte offsets in this requirement's quote
    pub incomplete: usize,  // offsets not yet covered
    pub citation: usize,    // offsets covered by Citation annotations
    pub implication: usize, // offsets covered by Implication annotations
    pub test: usize,        // offsets covered by Test annotations
    pub exception: usize,   // offsets covered by Exception annotations
    pub todo: usize,        // offsets marked with Todo annotations
    
    // Related annotations that provide coverage
    pub related: Vec<String>,  // stable IDs of non-SPEC annotations covering this requirement
}
```

This mirrors the current `Spec` struct from `status.rs` but uses stable string IDs instead of ephemeral `usize`. The `related` field links to the actual annotations providing coverage, enabling:

1. **Merge**: Union related sets from multiple packages
2. **Attribution**: Track which source files provide coverage for each requirement
3. **Drill-down**: Frontend can show "covered by annotation X from source Y"

### 2.2 Implement read/write for v2

- `duvet report --json-v2 path` for writing
- Merge command reads v2 format

### 2.3 Keep existing JSON format

Current `json.rs` remains for HTML frontend compatibility. v2 is for tooling/merging.

### 2.4 v1 vs v2 comparison

| Aspect | v1 | v2 |
|--------|----|----|
| **Serialization** | Streaming macros, write-only | Serde derive, read/write |
| **Annotation IDs** | Sequential `usize` (ephemeral) | Stable hash-based strings |
| **Blob links** | Per-annotation `blob_link` (from source config) | Per-annotation `blob_link` |
| **Version field** | None | `"version": "2.0"` |

**Structure differences:**

```
v1                                    v2
──                                    ──
blob_link: string                     version: string
issue_link: string                    blob_link: string
specifications: { ... }               issue_link: string
annotations: [ ... ]                  specifications: { ... }
statuses: { ... }                     annotations: [ ... ]
refs: [ ... ]                         coverage: { ... }  (renamed from statuses)
                                      refs: [ ... ]
```

**Annotation differences:**

```json
// v1 annotation
{
  "source": "src/lib.rs",
  "target_path": "https://example.com/rfc",
  "target_section": "section-2.1",
  "line": 42,
  "type": "CITATION",
  "blob_link": "https://..."  // per-annotation, from source config
  // no id, no package, no quote
}

// v2 annotation
{
  "id": "a1b2c3d4",           // NEW: stable ID
  "source": "src/lib.rs",
  "blob_link": "https://...",  // same as v1
  "target_path": "https://example.com/rfc",
  "target_section": "section-2.1",
  "quote": "MUST accept...",   // NEW: included for merge
  "type": "CITATION",
  "line": 42
}
```

**Line segment differences:**

```json
// v1 - annotation IDs are integers, compact array format
[[[0, 1], 48, "text here"]]

// v2 - annotation IDs are stable strings, object format
[{ "annotation_ids": ["A", "B"], "status_id": 48, "text": "text here" }]
```

**Coverage/statuses differences:**

```json
// v1 - keyed by integer ID
"statuses": {
  "0": { "spec": 65, "incomplete": 0, "related": [1, 2] }
}

// v2 - keyed by stable string ID
"coverage": {
  "a1b2c3d4": { "spec": 65, "incomplete": 0, "related": ["b2c3d4e5", "c3d4e5f6"] }
}
```

**Key behavioral differences:**

1. **Roundtripping**: v1 cannot be read back; v2 can be deserialized and re-serialized
2. **Merge**: v1 IDs collide across reports; v2 stable IDs allow union
3. **Self-contained**: v2 annotations include `quote`, making them portable for merge without re-parsing sources

---

## Phase 3: Merge Functionality

### 3.1 Implement `duvet merge` command

```
duvet merge \
  --input package-a/.duvet/report-v2.json \
  --input package-b/.duvet/report-v2.json \
  --output merged-report.json \
  --html merged-report.html \
  --snapshot merged-snapshot.txt
```

### 3.2 Merge logic

- **Specifications**: Union by target path (should be identical across packages)
- **Annotations**: Union by stable ID with package attribution
- **Coverage**: Merge stats per requirement; covered if ANY package provides coverage

### 3.3 Conflict handling

- Same annotation ID, different content → error/warning
- Same spec section, different text → use first, warn if different

---

## Phase 4: Frontend Updates

### 4.1 Update React frontend ✅ COMPLETED (commit 75acb7c4)

- Frontend now uses annotation's `blob_link` if present, falls back to global
- Implemented in `www/src/result.js` via `createBlobLinker()` function

### 4.2 Generate merged HTML

Either:
- Generate v1 JSON from merged v2 for existing frontend
- Update frontend to consume v2 directly

---

## Implementation Order

1. **Phase 2.1-2.2**: v2 JSON format (foundation, testable independently)
2. **Phase 1.2**: Stable annotation IDs (required for correct merge)
3. **Phase 3.1-3.2**: Basic merge command (working pipeline)
4. ~~**Phase 1.1 + 4.1**: Per-source blob-link support (correct HTML output)~~ ✅ **COMPLETED** (commit 75acb7c4)
5. **Phase 4.2**: Polish HTML output

---

## Additional Considerations

### Merge config format

Since per-source `blob-link` is now stored in annotations in the JSON report, the merge config no longer needs to specify blob-link per input:

```toml
[[input]]
path = "package-a/.duvet/report-v2.json"

[[input]]
path = "package-b/.duvet/report-v2.json"
```

Each package's `.duvet/config.toml` should specify `blob-link` in its `[[source]]` blocks, and these will be preserved in the merged report.

### Partial coverage tracking

Track per-package contribution: "package A covers 60% of section X, package B covers 40%". Current byte-offset model supports this; v2 format should preserve detail.

### Incremental merging

With many packages, merge incrementally (A+B → AB, AB+C → ABC). Stable IDs make this work.

### Spec version drift

If packages reference different spec versions, need strategy. Simplest: require identical specs, fail on mismatch.

---

## Appendix: Complete v2 JSON Schema

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Duvet Report v2",
  "type": "object",
  "required": ["version", "specifications", "annotations", "coverage", "refs"],
  "properties": {
    "version": {
      "type": "string",
      "const": "2.0"
    },
    "blob_link": {
      "type": "string",
      "description": "Default URL prefix for source file links"
    },
    "issue_link": {
      "type": "string",
      "description": "URL prefix for issue tracker links"
    },
    "specifications": {
      "type": "object",
      "description": "Map of specification target path to specification content",
      "additionalProperties": { "$ref": "#/$defs/Specification" }
    },
    "annotations": {
      "type": "array",
      "items": { "$ref": "#/$defs/Annotation" }
    },
    "coverage": {
      "type": "object",
      "description": "Map of SPEC annotation ID to coverage status",
      "additionalProperties": { "$ref": "#/$defs/CoverageStatus" }
    },
    "refs": {
      "type": "array",
      "description": "Lookup table for reference status combinations (indexed by status_id)",
      "items": { "$ref": "#/$defs/RefStatus" }
    }
  },
  "$defs": {
    "Specification": {
      "type": "object",
      "required": ["format", "sections"],
      "properties": {
        "title": { "type": "string" },
        "format": {
          "type": "string",
          "enum": ["ietf", "markdown"]
        },
        "sections": {
          "type": "array",
          "items": { "$ref": "#/$defs/Section" }
        }
      }
    },
    "Section": {
      "type": "object",
      "required": ["id", "title", "lines"],
      "properties": {
        "id": {
          "type": "string",
          "description": "Section identifier, e.g., 'section-2.1'"
        },
        "title": { "type": "string" },
        "lines": {
          "type": "array",
          "items": { "$ref": "#/$defs/Line" }
        },
        "requirements": {
          "type": "array",
          "description": "Stable IDs of SPEC annotations in this section",
          "items": { "type": "string" }
        }
      }
    },
    "Line": {
      "oneOf": [
        {
          "type": "string",
          "description": "Plain line with no annotation coverage"
        },
        {
          "type": "array",
          "description": "Segmented line split by annotation boundaries",
          "items": { "$ref": "#/$defs/LineSegment" }
        }
      ]
    },
    "LineSegment": {
      "type": "object",
      "required": ["annotation_ids", "status_id", "text"],
      "properties": {
        "annotation_ids": {
          "type": "array",
          "description": "Stable IDs of annotations covering this segment",
          "items": { "type": "string" }
        },
        "status_id": {
          "type": "integer",
          "description": "Index into refs table for coverage status"
        },
        "text": {
          "type": "string",
          "description": "The text content of this segment"
        }
      }
    },
    "Annotation": {
      "type": "object",
      "required": ["id", "source", "target_path"],
      "properties": {
        "id": {
          "type": "string",
          "description": "Stable hash-based identifier"
        },
        "source": {
          "type": "string",
          "description": "Source file path containing the annotation"
        },
        "blob_link": {
          "type": "string",
          "description": "Resolved URL for this annotation's source file"
        },
        "target_path": {
          "type": "string",
          "description": "Specification path (URL or file path)"
        },
        "target_section": {
          "type": "string",
          "description": "Section ID within the specification"
        },
        "quote": {
          "type": "string",
          "description": "Quoted text from the specification"
        },
        "type": {
          "type": "string",
          "enum": ["SPEC", "CITATION", "TEST", "IMPLICATION", "EXCEPTION", "TODO"],
          "default": "CITATION"
        },
        "level": {
          "type": "string",
          "enum": ["AUTO", "MAY", "SHOULD", "MUST"],
          "default": "AUTO"
        },
        "line": {
          "type": "integer",
          "description": "Line number in source file"
        },
        "comment": { "type": "string" },
        "feature": { "type": "string" },
        "tracking_issue": { "type": "string" },
        "tags": {
          "type": "array",
          "items": { "type": "string" }
        }
      }
    },
    "CoverageStatus": {
      "type": "object",
      "description": "Coverage statistics for a SPEC annotation",
      "properties": {
        "spec": {
          "type": "integer",
          "description": "Total byte offsets in this requirement"
        },
        "incomplete": {
          "type": "integer",
          "description": "Byte offsets not yet covered"
        },
        "citation": {
          "type": "integer",
          "description": "Byte offsets covered by Citation annotations"
        },
        "implication": {
          "type": "integer",
          "description": "Byte offsets covered by Implication annotations"
        },
        "test": {
          "type": "integer",
          "description": "Byte offsets covered by Test annotations"
        },
        "exception": {
          "type": "integer",
          "description": "Byte offsets covered by Exception annotations"
        },
        "todo": {
          "type": "integer",
          "description": "Byte offsets marked with Todo annotations"
        },
        "related": {
          "type": "array",
          "description": "Stable IDs of annotations providing coverage",
          "items": { "type": "string" }
        }
      }
    },
    "RefStatus": {
      "type": "object",
      "description": "Reference status flags for a line segment",
      "properties": {
        "spec": { "type": "boolean" },
        "citation": { "type": "boolean" },
        "implication": { "type": "boolean" },
        "test": { "type": "boolean" },
        "exception": { "type": "boolean" },
        "todo": { "type": "boolean" },
        "level": {
          "type": "string",
          "enum": ["AUTO", "MAY", "SHOULD", "MUST"]
        }
      }
    }
  }
}
```
