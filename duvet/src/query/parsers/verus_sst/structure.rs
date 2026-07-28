// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Obligation structure extracted from parsed SST logs.
//!
//! Per `design/witness/spec.md` §5.2, the parser MUST emit
//! *structure* — obligation nodes and reference edges — not just a
//! flat span map. Pass 1 (aggregate executability map) and pass 2
//! (per-discharge-unit closure) are both projections of the
//! [`ObligationGraph`] built here; see [`super::closure`].
//!
//! Node and edge identification, per spec §5.5 (grounded in the
//! 2026-07-26 SST POC):
//!
//! - **Nodes** are top-level `(@ "span" (FunctionSst ...))` blocks.
//!   Identity is the fully-qualified path in `:name (Fun :path X)`;
//!   the block's own head span is the node's *extent*.
//! - **Edges** are symbolic `(Fun :path X)` references anywhere
//!   inside a block (self-references excluded). Callee spans are NOT
//!   inlined in the referencing block, which is why closure must be
//!   a graph traversal.
//!
//! The same function's `FunctionSst` block appears in several
//! per-module log files (imported declarations are re-logged). The
//! builder unions spans and edges across duplicates and *requires*
//! extents to agree — a conflict would mean two different source
//! functions share a fully-qualified path, which would make node
//! identity unsound, so it is an error rather than a warning.

use super::sexpr::{parse_all, Sexpr, SexprError};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

/// A source extent: file plus inclusive line range.
///
/// Column information is parsed and discarded: the witness model is
/// line-granular (`files: Map<FilePath, line → status>`, spec §1.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub file: String,
    pub start_line: u32,
    pub end_line: u32,
}

impl Span {
    /// Parse a Verus span string:
    /// `<path>:<start_line>:<start_col>: <end_line>:<end_col> (#<ctxt>)`
    ///
    /// Returns `None` for strings that are not spans (the format is
    /// anchored on the trailing ` (#N)` macro-context marker and the
    /// `": "` separator, both of which every span in the golden
    /// corpus carries).
    pub fn parse(s: &str) -> Option<Span> {
        let body = &s[..s.rfind(" (#")?];
        let (left, right) = body.rsplit_once(": ")?;
        let mut left_parts = left.rsplitn(3, ':');
        let start_col: u32 = left_parts.next()?.parse().ok()?;
        let _ = start_col;
        let start_line: u32 = left_parts.next()?.parse().ok()?;
        let file = left_parts.next()?;
        let end_line: u32 = right.split(':').next()?.parse().ok()?;
        if file.is_empty() {
            return None;
        }
        Some(Span {
            file: file.to_string(),
            start_line,
            end_line,
        })
    }

    /// Whether `line` falls inside this extent.
    pub fn contains(&self, file: &str, line: u32) -> bool {
        self.file == file && self.start_line <= line && line <= self.end_line
    }

    /// Number of lines in the extent.
    pub fn line_count(&self) -> u64 {
        u64::from(self.end_line) - u64::from(self.start_line) + 1
    }
}

/// One prover obligation: a top-level `FunctionSst` block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObligationNode {
    /// Fully-qualified path from `:name (Fun :path X)`.
    pub name: String,
    /// The block's own head span.
    pub extent: Span,
    /// Every span inside the block, expanded to per-file line sets.
    /// Unfiltered: project-file filtering is a *view* applied by
    /// closure/aggregation, not baked into the structure.
    pub spans: BTreeMap<String, BTreeSet<u32>>,
    /// Symbolic `(Fun :path X)` references, self excluded.
    pub edges: BTreeSet<String>,
}

/// The parsed artifact of record: all obligation nodes, by name.
#[derive(Clone, Debug, Default)]
pub struct ObligationGraph {
    pub nodes: BTreeMap<String, ObligationNode>,
}

/// Structure-extraction error.
#[derive(Debug)]
pub enum StructureError {
    Sexpr(SexprError),
    /// A `FunctionSst` block whose head span string does not parse.
    /// Every node needs an extent (spec §5.5); the golden corpus has
    /// none of these, so hitting one means the format changed.
    MissingExtent {
        name: String,
        head: String,
    },
    /// The same fully-qualified name with two different extents.
    ExtentConflict {
        name: String,
        first: Span,
        second: Span,
    },
    /// A `FunctionSst` block with no `:name (Fun :path X)`.
    MissingName,
}

impl fmt::Display for StructureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StructureError::Sexpr(e) => write!(f, "s-expression error: {e}"),
            StructureError::MissingExtent { name, head } => {
                write!(f, "FunctionSst {name}: head span {head:?} does not parse")
            }
            StructureError::ExtentConflict {
                name,
                first,
                second,
            } => write!(
                f,
                "FunctionSst {name}: conflicting extents {first:?} vs {second:?}"
            ),
            StructureError::MissingName => {
                write!(f, "FunctionSst block without :name (Fun :path ...)")
            }
        }
    }
}

impl std::error::Error for StructureError {}

impl From<SexprError> for StructureError {
    fn from(e: SexprError) -> Self {
        StructureError::Sexpr(e)
    }
}

/// Extract the obligation nodes of one `*-sst.vir` module log.
///
/// Returns nodes in file order, *without* cross-module merging —
/// use [`ObligationGraph::merge`] to combine modules. (Keeping the
/// per-module list observable is what lets tests pin the raw block
/// count of the corpus independently of deduplication.)
pub fn parse_module(source: &str) -> Result<Vec<ObligationNode>, StructureError> {
    let top_level = parse_all(source)?;
    let mut nodes = Vec::new();

    for expr in &top_level {
        let Some((head_span, payload)) = as_at_node(expr) else {
            // `trait_impl`, `group`, `assoc_type_impl`, `trait`
            // top-levels and non-span @-nodes are not obligations.
            continue;
        };
        let Some(items) = payload.as_list() else {
            continue;
        };
        if items.first().and_then(Sexpr::as_atom) != Some("FunctionSst") {
            continue;
        }

        let name = function_name(items)
            .ok_or(StructureError::MissingName)?
            .to_string();
        let extent = Span::parse(head_span).ok_or_else(|| StructureError::MissingExtent {
            name: name.clone(),
            head: head_span.to_string(),
        })?;

        let mut spans: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
        let mut edges = BTreeSet::new();
        // Walk the whole top-level node (including its own head span)
        // with an explicit stack — expression trees in real logs are
        // deep enough that call recursion is not safe here either.
        //
        // Span collection is by *string shape*, not syntactic
        // position: the writer records locations in at least four
        // contexts — `(@ "span" ...)` declaration nodes,
        // `(@@ "span" ...)` expression nodes, `:span "span"` fields,
        // and `:spans ("span" ...)` lists (assert/label metadata).
        // Collecting only the @-forms was tried first and silently
        // lost 13 of the golden corpus's 2003 project lines (all in
        // types.rs, reachable only through `:span`/`:spans`). Every
        // span-shaped string in the block is part of what this
        // obligation's elaboration consulted; the span format
        // (`path:l:c: l:c (#n)`) is specific enough that
        // false-positive matches on ordinary string constants are
        // not a practical concern, and over-collection within the
        // block is the correct direction for consulted semantics.
        let mut work: Vec<&Sexpr> = vec![expr];
        while let Some(e) = work.pop() {
            match e {
                Sexpr::Str(s) => {
                    if let Some(span) = Span::parse(s) {
                        spans
                            .entry(span.file.clone())
                            .or_default()
                            .extend(span.start_line..=span.end_line);
                    }
                }
                Sexpr::List(list) => {
                    if let Some(target) = as_fun_path(list) {
                        if target != name {
                            edges.insert(target.to_string());
                        }
                    }
                    work.extend(list.iter());
                }
                Sexpr::Atom(_) => {}
            }
        }

        nodes.push(ObligationNode {
            name,
            extent,
            spans,
            edges,
        });
    }

    Ok(nodes)
}

impl ObligationGraph {
    /// Merge module node lists into one graph: spans and edges union
    /// across duplicate names; extents must agree (see module docs).
    pub fn merge(
        modules: impl IntoIterator<Item = Vec<ObligationNode>>,
    ) -> Result<Self, StructureError> {
        let mut graph = ObligationGraph::default();
        for module in modules {
            for node in module {
                match graph.nodes.get_mut(&node.name) {
                    None => {
                        graph.nodes.insert(node.name.clone(), node);
                    }
                    Some(existing) => {
                        if existing.extent != node.extent {
                            return Err(StructureError::ExtentConflict {
                                name: node.name,
                                first: existing.extent.clone(),
                                second: node.extent,
                            });
                        }
                        for (file, lines) in node.spans {
                            existing.spans.entry(file).or_default().extend(lines);
                        }
                        existing.edges.extend(node.edges);
                    }
                }
            }
        }
        Ok(graph)
    }

    /// Parse and merge several module log sources.
    pub fn from_sources<'a>(
        sources: impl IntoIterator<Item = &'a str>,
    ) -> Result<Self, StructureError> {
        let modules = sources
            .into_iter()
            .map(parse_module)
            .collect::<Result<Vec<_>, _>>()?;
        Self::merge(modules)
    }
}

/// Match `(@ "string" payload ...)` or `(@@ "string" payload ...)`;
/// return the string and payload.
///
/// The writer uses `@` for declaration-level location nodes (all
/// top-level blocks) and `@@` for expression-level ones (spans
/// inside bodies, `ensures` expressions, etc.). Both carry spans;
/// closure line sets would silently lose all expression-level
/// lines (~9% of the golden aggregate) if `@@` were missed.
fn as_at_node<'a, 'b>(expr: &'b Sexpr<'a>) -> Option<(&'a str, &'b Sexpr<'a>)> {
    let list = expr.as_list()?;
    let head = list.first()?.as_atom()?;
    if head != "@" && head != "@@" {
        return None;
    }
    let s = list.get(1)?.as_str()?;
    Some((s, list.get(2)?))
}

/// Match exactly `(Fun :path X)`; return X.
fn as_fun_path<'a>(list: &[Sexpr<'a>]) -> Option<&'a str> {
    match list {
        [Sexpr::Atom("Fun"), Sexpr::Atom(":path"), Sexpr::Atom(path)] => Some(path),
        _ => None,
    }
}

/// Find `:name (Fun :path X)` in a `FunctionSst` item list.
fn function_name<'a>(items: &[Sexpr<'a>]) -> Option<&'a str> {
    let pos = items.iter().position(|e| e.as_atom() == Some(":name"))?;
    as_fun_path(items.get(pos + 1)?.as_list()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_parse_forms() {
        let s = Span::parse("duvet-coverage/src/types.rs:22:1: 31:2 (#0)").unwrap();
        assert_eq!(
            s,
            Span {
                file: "duvet-coverage/src/types.rs".into(),
                start_line: 22,
                end_line: 31
            }
        );
        // Absolute registry paths (colons only in the position part).
        let s = Span::parse("/home/u/.cargo/registry/src/x/vstd-0.0.0/seq.rs:5:1: 6:2 (#4360)")
            .unwrap();
        assert_eq!(s.start_line, 5);
        assert_eq!(s.end_line, 6);
        // Non-spans.
        assert_eq!(Span::parse("no span"), None);
        assert_eq!(Span::parse(""), None);
        assert_eq!(Span::parse("x.rs:1:1: 2:2"), None); // no (#N)
    }

    const MINI: &str = r#"
;; functions
(@ "src/a.rs:10:1: 20:2 (#0)"
 (FunctionSst :name (Fun :path crate::alpha) :body
  (@@ "src/a.rs:25:5: 26:9 (#0)" (Call (Fun :path crate::beta)))
  :assert_meta (AssertId :spans ("src/a.rs:30:1: 30:9 (#0)"))
  :self_ref (Fun :path crate::alpha)))
(@ "src/b.rs:5:1: 8:2 (#0)"
 (FunctionSst :name (Fun :path crate::beta) :body ()))
(trait_impl (Fun :path crate::ignored))
(@ "src/c.rs:1:1: 2:2 (#0)" (Datatype :name (Dt Path crate::D)))
"#;

    #[test]
    fn nodes_edges_extents() {
        let nodes = parse_module(MINI).unwrap();
        assert_eq!(nodes.len(), 2, "Datatype and trait_impl are not nodes");

        let alpha = &nodes[0];
        assert_eq!(alpha.name, "crate::alpha");
        assert_eq!(alpha.extent.file, "src/a.rs");
        assert_eq!((alpha.extent.start_line, alpha.extent.end_line), (10, 20));
        // Head span 10..=20, expression-level (@@) span 25..=26, and
        // the bare `:spans (...)` string 30 — all three span-carrying
        // contexts contribute.
        let expected: BTreeSet<u32> = (10..=20).chain(25..=26).chain([30]).collect();
        assert_eq!(alpha.spans["src/a.rs"], expected);
        // Self-reference excluded; beta edge captured.
        assert_eq!(alpha.edges, BTreeSet::from(["crate::beta".to_string()]));

        assert_eq!(nodes[1].name, "crate::beta");
        assert!(nodes[1].edges.is_empty());
    }

    #[test]
    fn merge_unions_and_rejects_extent_conflicts() {
        let a = parse_module(MINI).unwrap();
        let b = parse_module(MINI).unwrap();
        let g = ObligationGraph::merge([a, b]).unwrap();
        assert_eq!(g.nodes.len(), 2);

        let conflicting = parse_module(
            r#"(@ "src/a.rs:99:1: 99:2 (#0)" (FunctionSst :name (Fun :path crate::alpha)))"#,
        )
        .unwrap();
        let err = ObligationGraph::merge([parse_module(MINI).unwrap(), conflicting]).unwrap_err();
        assert!(matches!(err, StructureError::ExtentConflict { .. }));
    }
}
