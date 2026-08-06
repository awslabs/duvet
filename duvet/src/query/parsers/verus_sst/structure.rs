// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Obligation structure extracted from parsed SST logs.
//!
//! Pass 1 (aggregate executability map) and pass 2
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

//= design/witness/spec.md#verus-producer
//# A flat span inventory of one block covers essentially
//# only its own extent, so the Verus producer MUST parse its
//# artifact once into a structure of obligation nodes and
//# reference edges and derive everything else from that
//# structure — parse-into-structure is a MUST, not an
//# optimization.

use super::sexpr::{parse_all, Sexpr, SexprError};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

/// A source extent: file plus inclusive line range.
///
/// Column information is parsed and discarded: the witness model is
/// line-granular (`files: Map<FilePath, line → status>`, spec §1.2).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
    // Test-oracle surface (independent rooting predicate the golden
    // tests check `select_units` against): no engine-path consumer
    // since the verified core took over unit selection.
    #[allow(dead_code)]
    pub fn contains(&self, file: &str, line: u32) -> bool {
        self.file == file && self.start_line <= line && line <= self.end_line
    }

    /// Number of lines in the extent.
    // Test-oracle surface: see `contains`.
    #[allow(dead_code)]
    pub fn line_count(&self) -> u64 {
        u64::from(self.end_line) - u64::from(self.start_line) + 1
    }
}

/// The kind of a discharge unit (spec §5.3, §5.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum UnitKind {
    /// The obligation's own head extent (fn/lemma header).
    Extent,
    /// One `:enss` clause entry.
    Ensures,
    /// One `LoopInv` node's `:inv` expression.
    LoopInvariant,
    /// One `Stm Assert` inside the obligation's proof check.
    ProofAssert,
}

impl UnitKind {
    /// Report-label token for span-identity labels.
    pub fn token(self) -> &'static str {
        match self {
            UnitKind::Extent => "extent",
            UnitKind::Ensures => "ensures",
            UnitKind::LoopInvariant => "loop_invariant",
            UnitKind::ProofAssert => "proof_assert",
        }
    }
}

/// A sub-function discharge unit recorded by the artifact: one
/// ensures clause, loop invariant, or proof assert, with its own
/// span (spec §5.5 — the artifact records a distinct span per
/// clause/invariant/assert).
///
/// `Extent` units are not stored here; the node's `extent` field is
/// that unit. `index` is the unit's position within its kind's
/// artifact order in the defining block, giving span-identity
/// labels a stable clause index.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClauseUnit {
    pub kind: UnitKind,
    pub index: usize,
    pub span: Span,
    /// `ProofNoteLabel :text` found inside the unit's expression,
    /// when the artifact records one. Never populated for
    /// `Ensures` units — see [`parse_module`].
    pub note: Option<String>,
}

/// One prover obligation: a top-level `FunctionSst` block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObligationNode {
    /// Fully-qualified path from `:name (Fun :path X)`.
    pub name: String,
    /// The block's own head span.
    pub extent: Span,
    /// Sub-function discharge units (ensures clauses, loop
    /// invariants, proof asserts), in kind-then-artifact order.
    pub units: Vec<ClauseUnit>,
    /// Every span inside the block, deduplicated to inclusive line
    /// ranges per file — the node's **span set**, the retained fact
    /// the fill projects. Two views derive from it and they are
    /// deliberately different (spec §5.2 vs §5.4): the
    /// aggregate/liveness view sweeps every range in full
    /// ([`super::closure::aggregate_map`], [`Self::elaborates`]),
    /// while the witness fill takes span START lines only
    /// ([`Self::fill_lines`]).
    /// Unfiltered: project-file filtering is a *view* applied by
    /// closure/aggregation, not baked into the structure.
    //= design/witness/spec.md#closure
    //= type=implementation
    //# The fill is a declared **projection** of the retained fact — the
    //# per-function consulted span set the producer parses and keeps —
    //# chosen for the consumer that exists today, positional annotation
    //# evaluation (`target ∈ fill`), for which it is lossless; a future
    //# consumer needing execution extents (runtime-map union, strength
    //# comparison) MUST extend the producer to deliver the retained
    //# spans rather than reinterpret the fill.
    pub span_ranges: BTreeMap<String, BTreeSet<(u32, u32)>>,
    /// Symbolic `(Fun :path X)` references, self excluded.
    pub edges: BTreeSet<String>,
}

impl ObligationNode {
    /// Whether any recorded span of this node covers `file:line` —
    /// the elaboration (liveness/not-proof-testable) view of the
    /// span set: full ranges, containers included (spec §5.2's
    /// aggregate semantics: "elaborated").
    pub fn elaborates(&self, file: &str, line: u32) -> bool {
        self.span_ranges
            .get(file)
            .is_some_and(|ranges| ranges.iter().any(|&(s, e)| s <= line && line <= e))
    }

    /// The node's witness-fill contribution: the start line of
    /// every span, per file (spec §5.4 — the hoisted
    /// rule, strict variant).
    ///
    /// Precisely: the span set is the deduplicated inclusive line
    /// ranges of every span-shaped string in the node's block
    /// (columns discarded — the witness model is line-granular),
    /// and the fill is the set of range START lines. Nesting depth
    /// is irrelevant: declaration spans contribute their header
    /// line exactly like statement and expression spans contribute
    /// theirs. Ranges containing no line (inverted spans) are
    /// excluded — they also never elaborate, so fills stay inside
    /// the aggregate by construction.
    ///
    /// Comment/blank exclusion is a theorem, not a rule computed
    /// here: spans anchor AST nodes, ordinary comments and blanks
    /// are not nodes, so no span begins on one. This function never
    /// reads source text; the theorem is pinned by the tripwire
    /// test over the golden artifacts and their checked-in sources.
    ///
    //= design/witness/spec.md#closure
    //= type=implementation
    //# a line enters a fill iff a span of a reached function begins on
    //# that line — every span, at every nesting depth, declaration
    //# spans included, so the header line of a wrapped signature is
    //# addressable.
    //= design/witness/spec.md#closure
    //= type=implementation
    //# Producers MUST NOT
    //# lexically classify lines at runtime, and MUST pin the theorem
    //# with a test that lexes checked-in fixture sources against the
    //# golden artifacts (guarding against macro-expansion span
    //# placement).
    //= design/witness/spec.md#closure
    //= type=implementation
    //# A doc-comment line MAY begin a span — doc comments
    //# are attribute nodes — and the fill records it honestly.
    //= design/witness/spec.md#closure
    //= type=implementation
    //# A line
    //# the verifier never elaborated — unverified code — appears in no
    //# span set and MUST NOT appear in any fill.
    pub fn fill_lines(&self) -> BTreeMap<&str, BTreeSet<u32>> {
        let mut out: BTreeMap<&str, BTreeSet<u32>> = BTreeMap::new();
        for (file, ranges) in &self.span_ranges {
            let lines: BTreeSet<u32> = ranges
                .iter()
                .copied()
                .filter(|&(start, end)| start <= end)
                .map(|(start, _)| start)
                .collect();
            if !lines.is_empty() {
                out.insert(file.as_str(), lines);
            }
        }
        out
    }
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
    /// A clause-kind unit (ensures clause, loop invariant, or proof
    /// assert) whose span string does not parse. Same posture as
    /// [`StructureError::MissingExtent`], for the same reason
    /// (spec §5.2): silently skipping the unit would make every
    /// annotation on its lines fall back to extent rooting — the
    /// hoisting §5.3 forbids — turning a format change into quietly
    /// wrong verdicts. The golden corpus has no such spans.
    MissingClauseSpan {
        name: String,
        kind: UnitKind,
        span: String,
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
            StructureError::MissingClauseSpan { name, kind, span } => {
                write!(
                    f,
                    "FunctionSst {name}: {} clause span {span:?} does not parse",
                    kind.token()
                )
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
//= design/witness/spec.md#verus-producer
//= type=implementation
//# - The Verus producer MUST treat `FunctionSst` blocks as closure
//# nodes and `Fun :path` references as edges,
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

        let units = clause_units(items, &name)?;

        let mut spans: BTreeMap<String, BTreeSet<(u32, u32)>> = BTreeMap::new();
        let mut edges = BTreeSet::new();
        // Walk the whole top-level node (including its own head span)
        // with an explicit stack — expression trees in real logs are
        // deep enough that call recursion is not safe here either.
        //
        // Span collection is by *grammar position*, not string
        // shape: the writer records locations in exactly four
        // contexts — `(@ "span" ...)` declaration nodes,
        // `(@@ "span" ...)` expression nodes, `:span "span"` fields,
        // and `:spans ("span" ...)` lists (assert/label metadata) —
        // and only strings at those positions are collected.
        // Collecting only the @-forms was tried first and silently
        // lost 13 of the golden corpus's project lines (all in
        // types.rs, reachable only through `:span`/`:spans`; the
        // corpus total is pinned by the aggregate golden test in
        // `tests.rs`), which is how the field contexts were found.
        //
        // A shape heuristic ("collect every string `Span::parse`
        // accepts") was used before this rule and is UNSOUND in the
        // fill direction: the artifact records user string
        // *constants* verbatim (`Exp Const (Constant StrSlice
        // "...")`, probed 2026-08-06 on Verus 0.2026.05.24.ecee80a),
        // so a span-shaped literal in a verified fn would enter the
        // span set, widen the witness fill, and could falsely
        // discharge a pair on a line the prover never elaborated.
        // Over-collection is fine for the liveness (aggregate) view
        // but fail-UNSAFE for discharge; the grammar rule's failure
        // direction is the safe one — a missed fifth context
        // under-fills (a loud false FAIL), and the pinned corpus
        // aggregate (1990 lines) trips on any collection delta.
        //
        // A string at a location position that fails `Span::parse`
        // is skipped, not an error: the corpus carries "no location"
        // heads on non-FunctionSst declarations, and extents/clause
        // spans (the positions where silent skipping would re-root
        // annotations) are already hard errors elsewhere
        // (`MissingExtent`, `MissingClauseSpan`).
        let mut collect = |s: &str| {
            if let Some(span) = Span::parse(s) {
                spans
                    .entry(span.file.clone())
                    .or_default()
                    .insert((span.start_line, span.end_line));
            }
        };
        let mut work: Vec<&Sexpr> = vec![expr];
        while let Some(e) = work.pop() {
            let Sexpr::List(list) = e else { continue };
            if let Some(target) = as_fun_path(list) {
                if target != name {
                    edges.insert(target.to_string());
                }
            }
            // Contexts 1 & 2: `(@ "span" ...)` / `(@@ "span" ...)`.
            // Matched structurally (head atom + string at index 1)
            // rather than via `as_at_node`, which also demands a
            // payload: the location grammar is the first two
            // elements; whether a payload follows is irrelevant.
            if let [head, Sexpr::Str(s), ..] = list.as_slice() {
                if matches!(head.as_atom(), Some("@" | "@@")) {
                    collect(s);
                }
            }
            // Contexts 3 & 4: `:span "span"` and `:spans ("span" ...)`.
            for pair in list.windows(2) {
                match pair[0].as_atom() {
                    Some(":span") => {
                        if let Some(s) = pair[1].as_str() {
                            collect(s);
                        }
                    }
                    Some(":spans") => {
                        if let Some(items) = pair[1].as_list() {
                            for item in items {
                                if let Some(s) = item.as_str() {
                                    collect(s);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            work.extend(list.iter());
        }

        nodes.push(ObligationNode {
            name,
            extent,
            units,
            span_ranges: spans,
            edges,
        });
    }

    Ok(nodes)
}

/// Extract the sub-function discharge units of one `FunctionSst`
/// item list.
///
/// A clause-kind node whose span string fails [`Span::parse`] is a
/// hard [`StructureError::MissingClauseSpan`] — the same posture as
/// a head span that fails (spec §5.2's abort-don't-skip rule):
/// skipping the unit would silently re-root its annotations at the
/// enclosing extent, the hoisting spec §5.3 forbids.
///
/// Artifact shapes (empirical, Verus 0.2026.05.24.ecee80a,
/// 2026-07-29):
///
/// - Ensures clauses: the `:enss` field is `(tuple (exp ...)
///   (exp ...))` — two clause lists whose entries are
///   `(@@ "span" (Exp ...))`, one entry per declared clause. Both
///   slots are consumed; clause indices run over the
///   concatenation.
/// - Loop invariants: `(LoopInv :at_entry _ :at_exit _ :inv
///   (@@ "span" (Exp ...)))` nodes; the unit span is the `:inv`
///   expression's span.
/// - Proof asserts: `(@ "span" (Stm Assert (id) _ ...))`
///   statements; the unit span is the statement's head span.
///
/// Loop invariants and asserts are collected from the
/// `:exec_proof_check` subtree only: the `:recommends_check`
/// subtree re-records the same statements for the recommends
/// query (plus generated "recommendation not met" asserts that are
/// not user proof elements), and imported re-logged declarations
/// carry `:exec_proof_check None`, so this scoping both prevents
/// duplicates and keeps generated recommends checks out of
/// `dom(du)`.
///
/// Unit labels are decided in `DischargeUnit::clause`
/// (spec §5.5 — the annotation lives there); this
/// extractor only carries the `ProofNoteLabel` text through, and
/// never for ensures clauses:
//= design/witness/spec.md#verus-producer
//= type=implementation
//# and MUST support
//# discharge units of all four kinds: obligation extents,
//# `:enss` clause spans, `LoopInv` spans, and proof-assert spans
//# (decisions.md, Decision 18).
//= design/witness/spec.md#verus-producer
//= type=implementation
//# Until the upstream `proof_note`-on-ensures defect is fixed,
//# ensures-clause labels MUST come from span identity.
fn clause_units(items: &[Sexpr<'_>], name: &str) -> Result<Vec<ClauseUnit>, StructureError> {
    let mut units = Vec::new();

    // Ensures clauses: both `:enss` tuple slots (inside the
    // `:decl (FuncDeclSst ...)` sub-structure), in order.
    let enss = field_value(items, ":decl")
        .and_then(Sexpr::as_list)
        .and_then(|decl| field_value(decl, ":enss"))
        .and_then(Sexpr::as_list);
    if let Some(enss) = enss {
        if enss.first().and_then(Sexpr::as_atom) == Some("tuple") {
            let mut index = 0usize;
            for slot in &enss[1..] {
                let Some(clauses) = slot.as_list() else {
                    continue;
                };
                for clause in clauses {
                    let Some((span_str, _)) = as_at_node(clause) else {
                        continue;
                    };
                    // Note never consumed for ensures (the spec §5.5
                    // hazard rule): span identity only.
                    push_unit(&mut units, name, UnitKind::Ensures, index, span_str, None)?;
                    index += 1;
                }
            }
        }
    }

    // Loop invariants and proof asserts: `:exec_proof_check` only.
    let Some(check) = field_value(items, ":exec_proof_check") else {
        return Ok(units);
    };
    let (mut inv_index, mut assert_index) = (0usize, 0usize);
    let mut work: Vec<&Sexpr> = vec![check];
    while let Some(e) = work.pop() {
        let Sexpr::List(list) = e else { continue };
        // (@ "span" (Stm Assert (id) None exp ...)) — user proof asserts.
        if let Some((span_str, payload)) = as_at_node(e) {
            if is_stm_assert(payload) {
                push_unit(
                    &mut units,
                    name,
                    UnitKind::ProofAssert,
                    assert_index,
                    span_str,
                    proof_note(payload),
                )?;
                assert_index += 1;
                // Do not descend: an assert's expression can carry
                // further span-shaped strings but no nested units.
                continue;
            }
        }
        // (LoopInv :at_entry _ :at_exit _ :inv (@@ "span" exp))
        if let [Sexpr::Atom("LoopInv"), rest @ ..] = list.as_slice() {
            if let Some(inv) = field_value(rest, ":inv") {
                if let Some((span_str, exp)) = as_at_node(inv) {
                    push_unit(
                        &mut units,
                        name,
                        UnitKind::LoopInvariant,
                        inv_index,
                        span_str,
                        proof_note(exp),
                    )?;
                    inv_index += 1;
                }
            }
        }
        // Reverse push so the worklist pops in document order —
        // unit indices are document-order positions.
        work.extend(list.iter().rev());
    }

    Ok(units)
}

/// Parse one clause-kind unit's span and push the unit — the shared
/// arm of ensures/invariant/assert collection in [`clause_units`]. A
/// span string that fails [`Span::parse`] is a hard
/// [`StructureError::MissingClauseSpan`] (spec §5.2's abort-don't-skip
/// posture; see [`clause_units`]'s docs for why skipping would re-root
/// annotations at the enclosing extent).
fn push_unit(
    units: &mut Vec<ClauseUnit>,
    name: &str,
    kind: UnitKind,
    index: usize,
    span_str: &str,
    note: Option<String>,
) -> Result<(), StructureError> {
    let span = Span::parse(span_str).ok_or_else(|| StructureError::MissingClauseSpan {
        name: name.to_string(),
        kind,
        span: span_str.to_string(),
    })?;
    units.push(ClauseUnit {
        kind,
        index,
        span,
        note,
    });
    Ok(())
}

/// Match a *user* proof assert: `(Stm Assert (id) None ...)`.
///
/// The fourth element is the generated-diagnostic slot: `None` for
/// user-written `assert(...)` statements, `(Message :level Error
/// :note "possible arithmetic underflow/overflow" ...)` (and
/// similar) for compiler-generated overflow/bounds/recommends
/// checks. Generated checks sit on executable body lines — proof
/// ingredients, not claims — so they are not discharge units
/// (spec §5.3: executable body lines MUST NOT be in the domain).
fn is_stm_assert(e: &Sexpr<'_>) -> bool {
    matches!(
        e.as_list(),
        Some([
            Sexpr::Atom("Stm"),
            Sexpr::Atom("Assert"),
            _,
            Sexpr::Atom("None"),
            ..
        ])
    )
}

/// First `(ProofNoteLabel :text "..." ...)` in an expression
/// subtree, depth-first.
///
/// Empirical shape (probe artifact, 2026-07-29): a noted
/// clause/assert wraps its expression as `(Exp UnaryOpr (UnaryOpr
/// ProofNote (ProofNoteLabel :text "..." :is_custom_err false))
/// inner)`, possibly below other operators.
fn proof_note(e: &Sexpr<'_>) -> Option<String> {
    let mut work: Vec<&Sexpr> = vec![e];
    while let Some(e) = work.pop() {
        let Sexpr::List(list) = e else { continue };
        if let [Sexpr::Atom("ProofNoteLabel"), rest @ ..] = list.as_slice() {
            if let Some(Sexpr::Str(text)) = field_value(rest, ":text") {
                return Some((*text).to_string());
            }
        }
        work.extend(list.iter());
    }
    None
}

/// Find the value following a `:field` atom in an item list.
fn field_value<'a, 'b>(items: &'b [Sexpr<'a>], field: &str) -> Option<&'b Sexpr<'a>> {
    let pos = items.iter().position(|e| e.as_atom() == Some(field))?;
    items.get(pos + 1)
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
                        for (file, ranges) in node.span_ranges {
                            existing.span_ranges.entry(file).or_default().extend(ranges);
                        }
                        existing.edges.extend(node.edges);
                        // Re-logged declarations repeat the same
                        // clause units (and only the defining module
                        // carries a proof check): union by value.
                        // ClauseUnit's derived Ord agrees with its
                        // derived Eq, so a sorted-set union equals the
                        // old contains-then-sort exactly, without the
                        // O(units²) membership scans.
                        let mut units: std::collections::BTreeSet<ClauseUnit> =
                            existing.units.drain(..).collect();
                        units.extend(node.units);
                        existing.units = units.into_iter().collect();
                    }
                }
            }
        }
        Ok(graph)
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
    as_fun_path(field_value(items, ":name")?.as_list()?)
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

    // `Span::parse` sits on the same trust boundary as the
    // S-expression parser: every string atom in an artifact is
    // offered to it. Total over arbitrary input: never panics (the
    // slice at `rfind(" (#")` and the rsplit arithmetic must hold
    // for any string, including multi-byte UTF-8), and on `Some`
    // the documented invariant holds — a non-empty file path.
    #[test]
    fn span_parse_total_over_arbitrary_input() {
        bolero::check!().with_type::<String>().for_each(|s| {
            if let Some(span) = Span::parse(s) {
                assert!(!span.file.is_empty(), "parsed span with empty file: {s:?}");
            }
        });
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
        // contexts contribute to the span set, as dedup'd line ranges.
        let expected: BTreeSet<(u32, u32)> = [(10, 20), (25, 26), (30, 30)].into();
        assert_eq!(alpha.span_ranges["src/a.rs"], expected);
        // The elaboration view sweeps ranges in full…
        assert!(alpha.elaborates("src/a.rs", 15));
        assert!(!alpha.elaborates("src/a.rs", 22));
        assert!(!alpha.elaborates("src/nope.rs", 15));
        // …while the fill takes each range's START line only. Both
        // are projections of the retained span set — the fact stays
        // available to future consumers needing real extents.
        //= design/witness/spec.md#closure
        //= type=test
        //# The fill is a declared **projection** of the retained fact — the
        //# per-function consulted span set the producer parses and keeps —
        //# chosen for the consumer that exists today, positional annotation
        //# evaluation (`target ∈ fill`), for which it is lossless; a future
        //# consumer needing execution extents (runtime-map union, strength
        //# comparison) MUST extend the producer to deliver the retained
        //# spans rather than reinterpret the fill.
        let fill: BTreeSet<u32> = [10, 25, 30].into();
        assert_eq!(alpha.fill_lines()["src/a.rs"], fill);
        // Self-reference excluded; beta edge captured.
        assert_eq!(alpha.edges, BTreeSet::from(["crate::beta".to_string()]));

        assert_eq!(nodes[1].name, "crate::beta");
        assert!(nodes[1].edges.is_empty());
    }

    #[test]
    fn fill_lines_start_lines_at_every_depth() {
        // A body-shaped nest: extent 10..=30 contains a body block
        // 12..=28, which contains three statement spans. Every span
        // contributes its START line — the extent's header line 10
        // and the block's opening line 12 included (declaration
        // spans are not special) — and nothing else: continuation
        // lines (14..=15, 22), the extent's interior gap lines
        // (16..=19, 23..=27), and the closing lines (29..=30) are
        // NOT in the fill because no span begins there.
        let src = r#"
(@ "src/a.rs:10:1: 30:2 (#0)"
 (FunctionSst :name (Fun :path crate::nested) :body
  (@ "src/a.rs:12:5: 28:6 (#0)" (Stm Block (
   (@@ "src/a.rs:13:9: 15:20 (#0)" (Exp One))
   (@@ "src/a.rs:20:9: 20:20 (#0)" (Exp Two))
   (@@ "src/a.rs:21:9: 22:20 (#0)" (Exp Three)))))))
"#;
        let nodes = parse_module(src).unwrap();
        //= design/witness/spec.md#closure
        //= type=test
        //# a line enters a fill iff a span of a reached function begins on
        //# that line — every span, at every nesting depth, declaration
        //# spans included, so the header line of a wrapped signature is
        //# addressable.
        let expected: BTreeSet<u32> = [10, 12, 13, 20, 21].into();
        assert_eq!(nodes[0].fill_lines()["src/a.rs"], expected);
        // The elaboration view still sweeps the full ranges.
        assert!(nodes[0].elaborates("src/a.rs", 17));
        assert!(nodes[0].elaborates("src/a.rs", 29));
    }

    #[test]
    fn fill_lines_edge_shapes() {
        use std::collections::BTreeMap;
        let node = |ranges: &[(u32, u32)]| ObligationNode {
            name: "c::x".into(),
            extent: Span {
                file: "f.rs".into(),
                start_line: 1,
                end_line: 1,
            },
            units: Vec::new(),
            span_ranges: BTreeMap::from([(
                "f.rs".to_string(),
                ranges.iter().copied().collect::<BTreeSet<_>>(),
            )]),
            edges: BTreeSet::new(),
        };
        let fill = |ranges: &[(u32, u32)]| -> BTreeSet<u32> {
            node(ranges).fill_lines().remove("f.rs").unwrap_or_default()
        };
        // Single-line span: its start line (single-line
        // signature-only obligations stay addressable).
        assert_eq!(fill(&[(5, 5)]), [5].into());
        // Nesting is irrelevant — container and containee both
        // contribute their start lines; shared starts dedup.
        assert_eq!(fill(&[(10, 20), (10, 12)]), [10].into());
        assert_eq!(fill(&[(10, 20), (18, 20)]), [10, 18].into());
        assert_eq!(fill(&[(1, 30), (5, 20), (7, 9)]), [1, 5, 7].into());
        // Overlap without containment: both start lines.
        assert_eq!(fill(&[(10, 15), (12, 18)]), [10, 12].into());
        // Continuation lines of a multi-line span never enter.
        assert_eq!(fill(&[(3, 9)]), [3].into());
        // Inverted ranges contain no line and are excluded (they
        // also never elaborate, so fills stay inside the aggregate).
        assert_eq!(fill(&[(9, 3)]), BTreeSet::new());
        assert_eq!(fill(&[(9, 3), (5, 6)]), [5].into());
    }

    #[test]
    fn span_shaped_string_constant_is_not_collected() {
        // The fill-direction soundness rule: collection is by
        // grammar position, not string shape. The artifact records
        // user string constants verbatim — probed 2026-08-06 on
        // Verus 0.2026.05.24.ecee80a: `let s = "src/engine.rs:100:1:
        // 200:2 (#0)";` in a verified fn appears in the SST as
        // `(Exp Const (Constant StrSlice "src/engine.rs:100:1:
        // 200:2 (#0)"))`. Under shape-based collection that literal
        // enters the span set, widens the fill with
        // src/engine.rs:100, and can FALSELY DISCHARGE a pair on a
        // line the prover never elaborated. Under the grammar rule
        // it is at payload position — not index 1 of an @/@@ node,
        // not a `:span`/`:spans` field value — and is not collected.
        let src = r#"
(@ "src/a.rs:10:1: 20:2 (#0)"
 (FunctionSst :name (Fun :path crate::holds_literal) :body
  (@@ "src/a.rs:12:5: 12:46 (#0)"
   (Exp Const (Constant StrSlice "src/engine.rs:100:1: 200:2 (#0)")))))
"#;
        let nodes = parse_module(src).unwrap();
        let node = &nodes[0];
        // The genuine location contexts are collected...
        let expected: BTreeSet<(u32, u32)> = [(10, 20), (12, 12)].into();
        assert_eq!(node.span_ranges["src/a.rs"], expected);
        // ...and the span-shaped constant contributes nothing: no
        // src/engine.rs entry, so no fill line, no elaboration, no
        // false discharge surface.
        assert!(
            !node.span_ranges.contains_key("src/engine.rs"),
            "span-shaped string constant leaked into the span set"
        );
        assert!(node.fill_lines().get("src/engine.rs").is_none());
        assert!(!node.elaborates("src/engine.rs", 100));
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

    // A block with every unit-kind shape the artifact records
    // (spec §5.5): two ensures clauses split across the two
    // `:enss` tuple slots, a noted and an unnoted loop invariant,
    // a noted user assert, and a generated (Message-slot) assert.
    const UNITS: &str = r#"
(@ "src/u.rs:10:1: 12:2 (#0)"
 (FunctionSst :name (Fun :path crate::units)
  :decl (FuncDeclSst :reqs ()
   :enss (tuple
    ((@@ "src/u.rs:20:9: 21:30 (#0)"
      (Exp UnaryOpr
       (UnaryOpr ProofNote (ProofNoteLabel :text "smuggled" :is_custom_err false))
       (Exp Const (Constant Bool true)))))
    ((@@ "src/u.rs:22:9: 22:30 (#0)" (Exp Const (Constant Bool true))))))
  :exec_proof_check (FuncCheckSst :reqs ()
   :body (@ "src/u.rs:13:1: 40:2 (#0)" (Stm Block (
    (Loop :invs (
      (LoopInv :at_entry true :at_exit true :inv
       (@@ "src/u.rs:30:13: 30:20 (#0)"
        (Exp UnaryOpr
         (UnaryOpr ProofNote (ProofNoteLabel :text "inv note" :is_custom_err false))
         (Exp Const (Constant Bool true)))))
      (LoopInv :at_entry true :at_exit true :inv
       (@@ "src/u.rs:31:13: 31:20 (#0)" (Exp Const (Constant Bool true))))))
    (@ "src/u.rs:35:12: 35:40 (#0)"
     (Stm Assert (0) None
      (@@ "src/u.rs:35:12: 35:40 (#0)"
       (Exp UnaryOpr
        (UnaryOpr ProofNote (ProofNoteLabel :text "assert note" :is_custom_err false))
        (Exp Const (Constant Bool true))))))
    (@ "src/u.rs:36:5: 36:20 (#0)"
     (Stm Assert (1) (Message :level Error :note "possible arithmetic underflow/overflow")
      (@@ "src/u.rs:36:5: 36:20 (#0)" (Exp Const (Constant Bool true)))))))))
  :recommends_check (FuncCheckSst :reqs ()
   :body (@ "src/u.rs:13:1: 40:2 (#0)" (Stm Block (
    (Loop :invs (
      (LoopInv :at_entry true :at_exit true :inv
       (@@ "src/u.rs:30:13: 30:20 (#0)" (Exp Const (Constant Bool true))))))))))))
"#;

    // A clause-kind span that fails `Span::parse` must be as loud
    // as a head span that fails (spec §5.2: silently dropping a
    // position would convert a format change into extent-hoisted /
    // missing-witness verdicts — the hoisting §5.3 forbids). The
    // golden corpus has no such spans (its 113 "no location"
    // strings are all on non-FunctionSst declaration heads), so
    // hitting one means the format changed.

    #[test]
    fn malformed_ensures_span_is_a_hard_error() {
        let src = r#"
(@ "src/u.rs:10:1: 12:2 (#0)"
 (FunctionSst :name (Fun :path crate::bad)
  :decl (FuncDeclSst :reqs ()
   :enss (tuple
    ((@@ "no location" (Exp Const (Constant Bool true))))
    ()))))
"#;
        let err = parse_module(src).unwrap_err();
        assert!(
            matches!(
                &err,
                StructureError::MissingClauseSpan { name, kind, span }
                    if name == "crate::bad"
                        && *kind == UnitKind::Ensures
                        && span == "no location"
            ),
            "expected MissingClauseSpan, got: {err}"
        );
    }

    #[test]
    fn malformed_loop_invariant_span_is_a_hard_error() {
        let src = r#"
(@ "src/u.rs:10:1: 12:2 (#0)"
 (FunctionSst :name (Fun :path crate::bad)
  :exec_proof_check (FuncCheckSst :reqs ()
   :body (@ "src/u.rs:13:1: 40:2 (#0)" (Stm Block (
    (Loop :invs (
      (LoopInv :at_entry true :at_exit true :inv
       (@@ "no location" (Exp Const (Constant Bool true))))))))))))
"#;
        let err = parse_module(src).unwrap_err();
        assert!(
            matches!(
                &err,
                StructureError::MissingClauseSpan { kind, .. }
                    if *kind == UnitKind::LoopInvariant
            ),
            "expected MissingClauseSpan, got: {err}"
        );
    }

    #[test]
    fn malformed_proof_assert_span_is_a_hard_error() {
        let src = r#"
(@ "src/u.rs:10:1: 12:2 (#0)"
 (FunctionSst :name (Fun :path crate::bad)
  :exec_proof_check (FuncCheckSst :reqs ()
   :body (@ "src/u.rs:13:1: 40:2 (#0)" (Stm Block (
    (@ "no location"
     (Stm Assert (0) None
      (@@ "src/u.rs:35:12: 35:40 (#0)" (Exp Const (Constant Bool true)))))))))))
"#;
        let err = parse_module(src).unwrap_err();
        assert!(
            matches!(
                &err,
                StructureError::MissingClauseSpan { kind, .. }
                    if *kind == UnitKind::ProofAssert
            ),
            "expected MissingClauseSpan, got: {err}"
        );
    }

    #[test]
    fn clause_units_of_every_kind() {
        let nodes = parse_module(UNITS).unwrap();
        let units = &nodes[0].units;
        assert_eq!(
            units
                .iter()
                .map(|u| (u.kind, u.index, u.span.start_line, u.note.as_deref()))
                .collect::<Vec<_>>(),
            [
                // Both `:enss` tuple slots contribute, indices over
                // the concatenation; the smuggled ProofNoteLabel is
                // NOT consumed for ensures (spec §5.5 hazard
                // rule: span identity only).
                (UnitKind::Ensures, 0, 20, None),
                (UnitKind::Ensures, 1, 22, None),
                // Loop invariants and asserts come from
                // :exec_proof_check only — the :recommends_check
                // copy of the invariant does not duplicate, and the
                // generated Message-slot assert is not a unit.
                (UnitKind::LoopInvariant, 0, 30, Some("inv note")),
                (UnitKind::LoopInvariant, 1, 31, None),
                (UnitKind::ProofAssert, 0, 35, Some("assert note")),
            ]
        );
    }
}
