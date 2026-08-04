# Producer core: verified-model properties

This document is the normative companion to
[spec.md §5](spec.md#prover-producers) for the *verified* producer
core.
[spec.md §5.3](spec.md#discharge-unit)–[§5.5](spec.md#verus-producer)
define what the Verus producer must do; this document states the
properties that MUST be proven with Verus, as a new phase of the
verified coverage model, over a verified model of the parsed
obligation graph. The properties are stated over the model's
vocabulary only — graphs as adjacency over opaque node ids, spans
as per-file line sets; no artifact syntax appears.

The Verus proof files MUST carry duvet annotations citing the
anchors in this document.

## 1. Model {#model}

The verified model of the parsed obligation graph
([spec §5.5](spec.md#verus-producer) — "parse-into-structure is a
MUST"):

- **Nodes** are opaque ids `0..n`, one per `FunctionSst` block.
- **Edges** are the symbolic references that resolve to a block in
  the artifact; references to names with no block are not part of
  the obligation graph ([spec §5.4](spec.md#closure)) and MUST NOT
  appear as model edges.
- **Spans** are per-node `(file id, line)` sets, the parsed source
  extents of everything the node's elaboration consulted.
- **Project membership** is a per-file-id boolean view supplied by
  the engine's project predicate.

Glue assumptions, named per the
[§4.4 pattern](spec.md#engine-glue) and NOT verified here:

- **PG1 (graph translation).** The adapter constructs the model
  faithfully from the parsed structure: injective node-id
  assignment, exactly the resolving edges, and per-node span rows
  materialized under the fill rule of
  [spec §5.4](spec.md#closure) (span-start lines,
  decisions.md Decision 19) — the model proves the union over the
  reached set; which lines each node contributes is the adapter's
  glue.
  Checked by the golden corpus ([spec §4.3](spec.md#obligation-testing));
  this is the SST-grammar-faithfulness residue of the trusted base.
- **PG2 (call obligation).** The producer computes every closure,
  rooting decision, and witness line set by calling this layer's
  functions; no parallel computation exists.
- **PG3 (project view).** The per-file-id project booleans agree
  with the engine's project predicate.

## 2. Properties {#properties}

### Property P1: Closure Fixpoint {#property-p1-closure-fixpoint}

The implementation MUST prove that the computed closure of a
discharge unit's root is exactly the downward-reachable set of the
obligation graph — the least fixpoint of the edge relation
containing the root:

```
t ∈ closure_reached(graph, root)  ⟺  reachable(graph, root, t)
```

where `reachable` is the reflexive-transitive reach along reference
edges. Both inclusions are load-bearing: completeness — every
reachable node enters, through any number of reference hops, with
no truncation at a depth bound
([spec §5.4](spec.md#closure)'s fixpoint requirement, reflexivity
included: `reachable(graph, root, root)` holds by zero hops) — and
transparency — no node outside the reachable set may be included
([spec §5.4](spec.md#closure): "nothing outside the reachable set
may be included").

### Property P2: Most-Specific-Wins Rooting {#property-p2-most-specific-wins}

The implementation MUST prove that the units selected for a
position are exactly the minimal-extent containing units at the
finest populated specificity level
([spec §5.3](spec.md#discharge-unit),
decisions.md [Decision 20](decisions.md#decision-20)):

```
selected(u, pos)  ⟺  contains(u, pos)
                     ∧ level(u) = finest level containing pos
                     ∧ extent(u) minimal among that level's
                       containing units
```

Consequences the proof MUST deliver: every selected unit contains
the position; no containing unit at a strictly finer specificity
level exists when an extent-level unit is selected; and ties at the
winning level and minimal extent are all selected — never chosen
among (decisions.md, [Decision 12](decisions.md#decision-12)).

### Property P3: Witness Assembly {#property-p3-witness-assembly}

The implementation MUST prove that a witness's line set equals the
union of the closure's per-file spans restricted to project files:

```
(f, l) ∈ witness_files(graph, root, project)
    ⟺  project(f)
        ∧ ∃ t ∈ closure_reached(graph, root) : (f, l) ∈ spans(t)
```

The verified assembly function's inputs are the graph, the root
obligation, and the project view only: the delivered witness is a
function of (artifact, obligation) alone, carrying no annotation
identity — that requirement is
[spec §1.7](spec.md#producer)'s and is discharged here by the
model's vocabulary (no annotation appears in it), not restated.

### Property P4: Filter Soundness {#property-p4-filter-soundness}

The implementation MUST prove that project filtering is a view,
not a truncation (decisions.md,
[Decision 7](decisions.md#decision-7)'s consulted semantics;
[spec §5.4](spec.md#closure)'s unfiltered traversal):

```
witness_files(graph, root, project)
    = witness_files(graph, root, ⊤) ∩ project files
```

Filtering removes only non-project lines: every project-file line
of the unfiltered assembly survives, and nothing else appears.
Edge traversal MUST NOT be filtered — `closure_reached` takes no
project view, so the reached set is project-independent by the
model's construction, and only the span projection is restricted.
