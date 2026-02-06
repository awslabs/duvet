# Duvet Configuration Format

Context document describing the TOML configuration format and its relationship to internal data structures.

## Configuration File Location

Default path: `.duvet/config.toml` (relative to current working directory)

## Schema Version

Current schema: `v0.4.0`
- JSON Schema: `https://awslabs.github.io/duvet/config/v0.4.0.json`
- Required `$schema` field for validation

## Internal Data Model

### Core Config Structure (duvet/src/config.rs)

```
Config
├── sources: Vec<Source>              # Source file patterns to scan
├── requirements: Vec<Requirement>    # Requirement file patterns
├── specifications: Vec<Specification> # Specification documents
├── report: Report                    # Report generation settings
├── requirements_path: Path           # Where extracted requirements are stored
└── download_path: Path               # Where downloaded specs are cached
```

### Source Configuration

```
Source
├── pattern: String                   # Glob pattern (e.g., "src/**/*.rs")
├── root: Path                        # Base directory (derived from config location)
├── comment_style: comment::Pattern   # Comment parsing configuration
├── default_type: AnnotationType      # Default annotation type for this source
└── blob_link: Option<Arc<str>>       # Optional per-source blob link (overrides report.html.blob-link)
```

### Requirement Configuration

```
Requirement
├── pattern: String                   # Glob pattern for requirement files
└── root: Path                        # Base directory (derived from config location)
```

### Specification Configuration

```
Specification
└── target: Arc<Target>               # Specification source and format

Target (duvet/src/target.rs)
├── path: TargetPath                  # URL or file path (from TOML "source" field)
└── format: Format                    # Parsing format (Auto, Ietf, Markdown)

TargetPath (enum)
├── Url(Url)                          # HTTP/HTTPS URL
└── Path(Path)                        # Local file path
```

### Report Configuration

```
Report
├── html: HtmlReport
├── json: JsonReport
└── snapshot: SnapshotReport

HtmlReport
├── enabled: bool                     # Generate HTML report
├── path: Path                        # Output file path
├── blob_link: Option<Arc<str>>       # URL prefix for source file links
└── issue_link: Option<Arc<str>>      # URL prefix for issue tracker links

JsonReport
├── enabled: bool                     # Generate JSON report
└── path: Path                        # Output file path

SnapshotReport
├── enabled: bool                     # Generate snapshot report
└── path: Path                        # Output file path
```

---

## TOML Configuration Format

File: `duvet/src/config/schema/v0_4_0.rs`

### Top-Level Structure

```toml
'$schema' = "https://awslabs.github.io/duvet/config/v0.4.0.json"

[[source]]
# Source file configuration (array)

[[requirement]]
# Additional requirement file patterns (array)

[[specification]]
# Specification documents (array)

[report]
# Report generation settings
```

### Source Configuration

Defines which source files to scan for annotations.

```toml
[[source]]
pattern = "src/**/*.rs"              # Required: glob pattern
type = "implementation"              # Optional: default annotation type
comment-style = { meta = "//=", content = "//#" }  # Optional: comment parsing
blob-link = "https://github.com/org/repo/blob/main"  # Optional: overrides report.html.blob-link for this source
```

**Supported Types** (TOML field: `type`):
- `implementation` (default) - Maps to `Citation` annotation type internally
- `spec` - Specification requirements
- `test` - Test annotations
- `exception` - Exception annotations
- `todo` - Todo annotations
- `implication` - Implication annotations

Note: The annotation parser also accepts `citation` as an alias for `implementation`.

**Comment Style** (TOML field: `comment-style`):
- `meta`: Prefix for annotation metadata (default: `"//="`)
- `content`: Prefix for quoted content (default: `"//#"`)

Note: Different languages may use different comment styles when initialized via `duvet init`:
- C: `meta = "*="`, `content = "*#"`
- Python: `meta = "##="`, `content = "##%"`
- Ruby: `meta = "##="`, `content = "##%"`
- Go, Java, JavaScript, TypeScript, Rust: use defaults (`//=`, `//#`)

**Blob Link** (TOML field: `blob-link`):
- Optional URL prefix for source file links in reports
- Overrides the global `report.html.blob-link` for annotations from this source pattern
- Supports the same template string syntax as the global blob-link (e.g., `${{ GITHUB_REF || 'main' }}`)
- Resolution: annotation uses its source's `blob-link` if present, otherwise falls back to `report.html.blob-link`

### Requirement Configuration

Additional patterns for requirement files (beyond defaults).

```toml
[[requirement]]
pattern = "custom/requirements/**/*.toml"  # Required: glob pattern
```

**Default Patterns** (automatically included):
- `.duvet/requirements/**/*.toml`
- `.duvet/todos/**/*.toml`
- `.duvet/exceptions/**/*.toml`

### Specification Configuration

Defines specification documents to process.

```toml
[[specification]]
source = "https://www.rfc-editor.org/rfc/rfc9000"  # URL or file path (required in practice)
format = "ietf"                                    # Optional: parsing format
```

**Supported Formats:**
- `ietf` (alias: `IETF`) - IETF RFC format
- `markdown` (aliases: `md`) - Markdown format
- Auto-detected from file extension or content if not specified

**Source Types:**
- HTTP/HTTPS URLs (downloaded and cached)
- Local file paths (relative to config file)

### Report Configuration

Controls report generation and output paths.

```toml
[report]
  [report.html]
  enabled = true                                    # Default: true
  path = ".duvet/reports/report.html"              # Default path
  blob-link = "https://github.com/${{ GITHUB_REPO }}/blob/${{ GITHUB_REF || 'main' }}"  # Optional
  issue-link = "https://github.com/${{ GITHUB_REPO }}/issues"   # Optional

  [report.json]
  enabled = false                                   # Default: false
  path = ".duvet/reports/report.json"              # Default path

  [report.snapshot]
  enabled = false                                   # Default: false
  path = ".duvet/snapshot.txt"                     # Default path
```

**Link Templates:**
- Support environment variable substitution: `${{ VAR }}`
- Support fallback values: `${{ VAR || 'default' }}`
- Whitespace around variable names and `||` is trimmed
- All variables are resolved from environment at config load time

---

## Path Resolution

### Download and Requirements Paths

Paths are derived from the config file location (not configurable in TOML):

- **Download path**: `.duvet/specifications/` (relative to config file)
- **Requirements path**: `.duvet/requirements/` (relative to config file)

### Pattern Resolution

All glob patterns are resolved relative to the project root directory (parent of `.duvet/`):

```toml
[[source]]
pattern = "src/**/*.rs"  # Matches: <project_root>/src/**/*.rs
```

### Output Paths

Report paths are relative to the project root:

```toml
[report.html]
path = ".duvet/reports/report.html"  # Resolves to: <project_root>/.duvet/reports/report.html
```

---

## Configuration Loading Process

File: `duvet/src/config.rs`

```
1. Discover config file (.duvet/config.toml)
2. Parse TOML → Schema enum (versioned)
3. Load sources, requirements, specifications
4. Resolve all paths relative to config location
5. Build internal Config struct
```

### Schema Versioning

The schema enum maps URL to internal version:

```rust
#[serde(tag = "$schema")]
enum Schema {
    #[serde(rename = "https://awslabs.github.io/duvet/config/v0.4.0.json")]
    V1_0_0(v0_4_0::Schema),  // Note: enum variant name differs from schema version
}
```

**Supported Schema URLs:**
- `https://awslabs.github.io/duvet/config/v0.4.0.json`
- `https://awslabs.github.io/duvet/config/v0.4.0.json#`
- `https://awslabs.github.io/duvet/config/v0.4.json`
- `https://awslabs.github.io/duvet/config/v0.4.json#`

### Default Configuration

When no config exists, `duvet init` creates:

```toml
'$schema' = "https://awslabs.github.io/duvet/config/v0.4.0.json"

[[source]]
pattern = "src/**/*.rs"  # Language-specific pattern (auto-detected or via --lang-* flags)

[[specification]]
source = "https://example.com/spec"  # User-provided via --specification flag

[report.html]
enabled = true

[report.snapshot]
enabled = true
```

Note: Language detection is automatic based on project files:
- C: `CMakeLists.txt`
- Go: `go.mod`
- Java: `pom.xml`, `build.gradle`, or `build.gradle.kts`
- JavaScript: `package.json`
- Python: `requirements.txt`, `pyproject.toml`, or `setup.py`
- TypeScript: `tsconfig.json`
- Ruby: `Gemfile` (uses `lib/**/*.rb` pattern)
- Rust: `Cargo.toml`

---

## Template String Processing

File: `duvet/src/config/schema.rs`

Link templates support environment variable substitution:

### Syntax

```
${{ VARIABLE }}                    # Environment variable (whitespace is trimmed)
${{ VARIABLE || 'fallback' }}      # With fallback value
${{ VAR1 || VAR2 || 'default' }}   # Multiple fallbacks
```

### Processing

1. Split on `${{` delimiters
2. Extract expression between `${{` and `}}`
3. Evaluate choices separated by `||`:
   - Whitespace around each choice is trimmed
   - Quoted strings: `'value'` → literal value
   - Unquoted names: environment variable lookup
4. Use first successful match (error if no match found)

Note: Template evaluation happens at config load time, not at report generation. All substitutions must be environment variables - there are no special built-in placeholders.

### Example

```toml
[report.html]
blob-link = "https://github.com/${{ GITHUB_REPO }}/blob/${{ GITHUB_REF || 'main' }}"
```

With environment:
- `GITHUB_REPO=user/project`
- `GITHUB_REF` unset

Result: `https://github.com/user/project/blob/main`

### How blob-link Works

The `blob-link` value is used as a URL prefix. The frontend automatically appends the source file path and line numbers when generating links. For example:

- Config: `blob-link = "https://github.com/user/project/blob/main"`
- Source file: `src/lib.rs` at line 42
- Generated link: `https://github.com/user/project/blob/main/src/lib.rs#L42`

For annotations spanning multiple lines, the link includes a range (e.g., `#L42-L50`).

### How issue-link Works

The `issue-link` value is used as a URL prefix for linking to issues. If an annotation references an issue number, the frontend generates a link by appending the issue identifier. Full URLs in issue references are used as-is.

---

## Configuration Validation

### JSON Schema

Generated from Rust types using `schemars`:
- Validates structure and types
- Enforces required fields
- Provides IDE autocompletion

### Runtime Validation

- Glob pattern compilation
- Path resolution
- URL parsing for specifications
- Template string evaluation

### Error Handling

Configuration errors include:
- Invalid TOML syntax
- Schema validation failures
- Invalid glob patterns
- Unresolvable template variables
- Missing specification sources

---

## Integration with Duvet Commands

### `duvet init`

Creates default configuration:
1. Detects project language (or uses `--lang-*` flags)
2. Sets appropriate source patterns
3. Adds specifications from `--specification` flags
4. Writes `.duvet/config.toml` and `.duvet/.gitignore`

### `duvet report`

Uses configuration to:
1. Scan source files matching patterns
2. Load requirement files
3. Download/parse specifications
4. Generate enabled report formats

### `duvet extract`

Extracts requirements from specifications:
1. Downloads specs to `download_path`
2. Parses based on format settings
3. Outputs TOML files to `requirements_path`

---

## Migration and Compatibility

### Schema Evolution

- The `$schema` field is required for config file parsing
- New fields may be added with sensible defaults in future versions
- Schema URL determines which parser version is used

### Version Detection

Schema version determined by `$schema` field:
- Missing schema → parse error (schema field is required for tag-based deserialization)
- Unknown schema URL → error

### Configuration Updates

Manual schema URL updates required for version migration when new schema versions are released.
