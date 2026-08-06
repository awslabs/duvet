# Kotlin Classifier: Decisions

This document records *why* the Kotlin line classifier maps tree-sitter node
kinds to `LineProperty` sets the way it does. The normative mapping lives in
[`kotlin-spec.md`](./kotlin-spec.md); this file is the evidence trail. The
language-generic contract both documents build on is
[`../query/coverage-model-spec.md`](../query/coverage-model-spec.md) §1.3
(classification function, `None` semantics, NonLinearControl and
mutual-exclusivity contracts), §1.5 (scope-stream balance or escalate), and §8
(extension-only dispatch, trust taxonomy).

## Method: corpus before code

Every decision below is grounded in a ground-truth corpus rather than syntax
theory. The corpus (thirteen Kotlin files exercising single-expression
functions, lambdas, `when`, string templates, labeled jumps, data classes,
inline functions, default arguments, suspend functions, try/catch, enums,
sealed hierarchies, and accessors) was compiled with kotlinc 2.4.10 (JVM
target), executed under the JaCoCo 0.8.15 agent with tests hitting known
subsets, and the XML report was correlated line-by-line with the sources. In
parallel, every corpus file was parsed with the chosen tree-sitter grammar and
the CSTs dumped. The classifier's job is to be consistent with where JaCoCo
*actually* places and fires line probes — the corpus is the arbiter wherever
naive expectation and observed behavior disagree.

JaCoCo evidence notation used below: `HIT`/`MISS`/`PART` are the per-line
verdicts derived from the XML `mi`/`ci` (missed/covered instruction) counts.

## Decision 1: grammar crate — `tree-sitter-kotlin-ng`

Two published crates parse Kotlin: `tree-sitter-kotlin` (fwcd's original
grammar) and `tree-sitter-kotlin-ng` (the tree-sitter-grammars organization's
maintained continuation of the same grammar). The workspace pins
`tree-sitter = "0.24"`. The fwcd crate's published versions pin an older
tree-sitter core; `tree-sitter-kotlin-ng 1.1.0` declares and compiles against
tree-sitter 0.24 (validated: the corpus CST dump was produced by a probe binary
built with `tree-sitter-kotlin-ng = "1.1.0"` and `tree-sitter = "0.24.7"` —
the exact core the workspace resolves). All thirteen corpus files parse with
zero `ERROR`/`MISSING` nodes.

Consequence: node-kind names in the spec are `-ng` names (`import`,
`enum_class_body`, `secondary_constructor`, `anonymous_initializer`, …).

## Decision 2: single-expression functions — the probe follows the expression

`fun f(x) = expr` has no block. Naive expectation: the probe lands on the
declaration line. Observed (SingleExpr.kt): when declaration and expression
share a line, that line probes (line 7 `fun exprCalled(x: Int) = x * 2` HIT;
line 9, the uncalled twin, MISS). When the expression sits on its own line, the
probe follows it and the declaration line has *no probe at all* (line 12
`fun exprSplitCalled(x: Int) =` unprobed; line 13 `x + 100` HIT). Multi-line
chain bodies probe on *each* chain line (lines 20–22).

Mapping: the `=`-form `function_body` span is `Statement` (probe-bearing);
declaration header lines are `Declaration` via the shared
declaration-with-scope marking. On the shared-line form the line ends up
`{Declaration, Statement}` — legal (the mutual-exclusivity contract only
separates Annotation/Comment/Whitespace from code properties) and matches the
Java `int x = 5;` precedent. No scope events: there is no block.

## Decision 3: function headers with default arguments are Declaration-only —
never Statement

Observed (Defaults.kt): default-value expressions carry their own probes, and
those probes fire only when the default is *evaluated*. A function called only
with all arguments supplied shows `MISS` on its header line even though the
function body ran (line 20 `fun allArgsProvided(a: Int = 10, b: Int = 20)`
MISS mi=14, while line 21 `return a + b` HIT). Multi-line parameter lists
probe per parameter line when defaults evaluate (lines 7–10 all HIT).

This forces the header mapping. If the header were `Statement`, phase-2
backward propagation would *stop* at it, and — since coverage reports it
missed — an annotation targeting the header would be `NotExecuted` even though
the function demonstrably ran. As `Declaration`, backward propagation from the
executed body crosses the header and the annotation resolves `Executed`. The
"defaults never evaluated" nuance is invisible to duvet, which asks "did the
implementation run", not "did every default evaluate" — the Declaration
mapping answers the right question. Parameter lines (with or without
defaults) are part of the header for the same reason.

## Decision 4: class-like headers carry synthesized-member probes —
Declaration via declaration-with-scope

Observed (DataClasses.kt, Enums.kt): JaCoCo attributes synthesized members to
the class header line, exactly like Java records. `data class UsedFully(...)`
PART (equals/hashCode/copy hit); `data class NeverInstantiated(...)` MISS
mi=9; sealed subclass headers likewise (`class Circle(val r: Double) :
Shape()` HIT, `class Square(...)` MISS); `object Unit_ : Shape()` HIT on
instantiation; `class NamedGreeter(...) : Greeter {` HIT (primary-constructor
probes live on the header).

Mapping: `class_declaration`, `object_declaration`, `companion_object` use the
Java `record_declaration` treatment — Declaration on header lines up to the
body, scope from the body child. A covered header is rescued into the exec set
directly (JaCoCo reports it covered); an uncovered one degrades to
NotExecuted/Structural via the verified phase-3 scope logic. No Statement on
headers (Decision 3's propagation argument applies identically).

## Decision 5: `when` — per-arm Statement, scope on the braces it carries
directly

Observed (WhenExpr.kt): each `when` arm probes independently (subject form:
line 8 `0 -> "zero"` HIT, line 9 `1 -> "one"` MISS, line 10 `else` MISS —
mirror of Java's arrow-form `switch_rule`, which is per-arm Statement).
Block-bodied arms probe the arm line *and* the block contents separately.

Grammar quirk with structural consequence: `when_expression` has **no block
child** — the `{` and `}` are direct token children of `when_expression`
itself (CST: `when_expression` → `when`, `when_subject`, `{`, `when_entry`*,
`}`). The same holds for `lambda_literal`. So scope events for these two kinds
are keyed on their own brace tokens, not on a child block node. (For
`lambda_literal` the braces are its first and last bytes, so the Java
byte-extremes pattern would work; `when_expression` starts at the `when`
keyword, so the general token-keyed form is required. The spec mandates
token-keyed for both — one rule, no special case.)

Mapping: `when_entry` → Statement on its start line; `when_expression` →
Statement on its start line (control header, Java `switch_expression`
precedent) plus ScopeOpen/ScopeClose on its brace-token lines.

## Decision 6: `do`/`while` — the probe is on the `} while (cond)` line

Observed (TryCatch.kt): `do {` (line 48) carries no probe; the condition line
`} while (i < n)` (line 51) is HIT with branch counters. Mapping Statement on
the *start* line (the Java `do_statement` pattern) would stamp a never-probed
line; instead `do_while_statement` marks Statement on its **end** line — which
is also the ScopeClose line of the body block, a legal compound. `while`
statements probe on their condition (start) line as expected (line 38 HIT) and
take the standard control-header treatment.

## Decision 7: NonLinearControl inventory — label declarations and labeled
jumps; `return@label` is a structured exit

The generic contract (§1.3) requires NonLinearControl on both source and
target of non-linear control flow, so that any scope enterable by jump
contains an NLC line and backward propagation is disabled there.

Kotlin inventory, following the Java precedent (`labeled_statement` → NLC at
java.rs; bare `break`/`continue` → Statement):

- **Label declaration** (`outer@ for (...)`, `lit@{ n -> ...}`): the `label`
  node → NonLinearControl on its line. This is the jump *target* side.
- **`break@label` / `continue@label`**: `labeled_expression` →
  NonLinearControl. These are jump *sources* naming a label.
- **`return@label`** (explicit or implicit label): `return_expression` whose
  first token is `return@` — observed CST shape `return_expression` →
  (`return@`, `identifier`). Mapped **Statement**, not NonLinearControl.
  Rationale: `return@label` exits the enclosing lambda early — the exact
  control shape of `continue`, which Java maps as Statement. The scope it
  leaves was entered normally (no scope is *entered* via the jump), so the
  propagation-soundness argument that motivates NLC does not apply to the
  source; the label-declaration line (when explicit) already carries NLC.
  Observed (Labels.kt): `return@lit` and `return@forEach` lines probe as
  ordinary statements (lines 36, 48 HIT ci=1).
- Plain `return` → Statement (structured exit; Java parity).

Cost acknowledged: a label-declaration line such as `xs.forEach lit@{ n ->`
(Labels.kt line 34, HIT) is classified NonLinearControl, and phase 3 resolves
NLC lines to `Unknown` — an annotation targeting exactly that line gets
`Unknown` rather than `Executed`. That is the contract's intended conservative
behavior for jump targets; annotations on the surrounding declaration or the
lambda body lines are unaffected.

## Decision 8: try/catch/finally headers — Declaration, Java parity

Observed (TryCatch.kt): `try {` probes on entry (line 9 HIT ci=1);
`} catch (...) {` probes when the handler runs (line 11 HIT); `} finally {`
never probes (line 13 unprobed, body probed). Same placement pattern as Java,
so the same mapping: `try_expression` start line, `catch_block` start line,
`finally_block` start line → Declaration; the `block` children carry the
scopes. A covered `try {` line enters the exec set from coverage directly;
propagation handles the rest. Kotlin's try-as-expression
(`val v = try { ... } catch ...`) changes nothing: the enclosing
`property_declaration` stamps the first line, the `try_expression` children
behave identically (lines 20–25 evidence).

## Decision 9: enum entries always probe — Declaration + Statement
unconditionally

Observed (Enums.kt): every `enum_entry` line probes, with or without
constructor arguments (`RED(0xFF0000)` HIT ci=7; argument-less `Plain.A`,
`B` HIT ci=6/13 — class initialization constructs all entries). This differs
from Java, where argument-less `enum_constant` gets Declaration only:
kotlinc emits per-entry initialization instructions attributed to the entry
line in all forms. Mapping: `enum_entry` → {Declaration, Statement}, no
argument-list condition. `enum_class_body` joins the scope-bearing set.

## Decision 10: properties — Statement iff initializer or delegate

`property_declaration` → Declaration over its span; plus Statement iff the
node carries an initializer (`= expr`) or a `property_delegate` (`by lazy
{...}` — observed probing on the delegate line, Extras.kt line 9 HIT).
Presence is asked of the grammar directly (an `=`-introduced value child or a
`property_delegate` child), mirroring the field-based
`node_has_initializer` approach java.rs uses instead of kind allowlists.
Evidence: `val settable: Int = 0` HIT (probe-bearing); bare `val stored: Int`
carries a (never-firing) probe but is correctly rescued by propagation as
Declaration (Extras.kt lines 14–18: init-block assignment HIT rescues the
header); `val computed: Int` with custom getter — unprobed, Declaration only,
getter body probes separately.

Accessor headers (`get() {`, `set(value) {`) never probe (Extras.kt lines
21, 26); bodies do (lines 22, 27). `getter`/`setter` take
declaration-with-scope. `init {` blocks *do* probe on the header line
(Extras.kt line 16 HIT ci=1); `anonymous_initializer` takes
declaration-with-scope (the Java `static_initializer` analog) and the covered
header enters the exec set from coverage.

## Decision 11: statement-position expressions — span marking, Java artifact
parity

The `-ng` grammar has no `expression_statement` wrapper: statements appear in
blocks directly as `call_expression`, `assignment`, `navigation_expression`,
etc. These kinds mark their span Statement (the Java expression-statement span
treatment). Nested occurrences re-mark lines already inside a marked span —
idempotent and harmless.

Two known conservative artifacts, both inherited from the Java template's
span-marking design, both acknowledged rather than special-cased:

- **Multi-line string templates**: the enclosing declaration span stamps
  Statement on every template line, but JaCoCo probes only lines containing
  interpolation or the literal's start/end (Templates.kt: plain-text lines 11,
  15 unprobed). An annotation targeting a plain-text template line resolves
  NotExecuted. Blank template lines are stamped Whitespace by the pre-pass and
  stripped to pure Whitespace by the verified post-pass — semantically they are
  string content, but JaCoCo never probes them either, and skipping them as
  targets is harmless.
- **Object-literal members inside call spans**:
  `startCoroutine(object : Continuation<Int> { override fun ... })` spans
  Statement across member-declaration lines that JaCoCo never probes
  (Suspend.kt line 47). Java has the identical artifact with anonymous classes
  inside expression statements.

## Decision 12: suspend functions need no special handling

Observed (Suspend.kt): the coroutine state-machine transform does *not*
distort line attribution — body lines probe on their own lines, suspension
points included (lines 14–17, 32–34 all HIT; the uncalled twin all MISS). A
single-expression suspend function follows Decision 2 (line 21 HIT on the
shared line). No suspend-specific mapping exists.

## Decision 13: inline functions keep their own probes

Observed (InlineFns.kt): inline function bodies retain probes attributed to
their declaration lines (called: lines 8–10 HIT; uncalled: lines 14–15 MISS;
reified: line 20 HIT) — kotlinc's inlining does not steal attribution to call
sites at the line level. No special handling.

## Decision 14: exception-propagating lines can read MISS — a JaCoCo
semantics note, not a classifier concern

JaCoCo inserts probes *after* instructions; a line that throws (or calls
something that throws) never reaches its probe, so it reports missed even
though it executed (TryCatch.kt line 76: `throwNotCaught(true)` inside a try —
MISS mi=4 despite having run and thrown). The classifier maps such lines
Statement like any other; the resulting NotExecuted verdict is a property of
JaCoCo's probe model that duvet inherits knowingly. Recorded here so the next
reader does not "fix" the classifier for it.

## Decision 15: parse errors defeat the commitment — Java parity

Identical rationale to java.rs (spec §1.5): `has_error()` on the root →
`Unclassifiable` with every `ERROR`/`MISSING` node's line reported. A
tree-sitter grammar gap and a not-actually-Kotlin file are indistinguishable
here; the classifier reports facts (where), never a cause (why), and the
dispatcher owns the response. The corpus parses clean (0 errors across all 13
files), so grammar-gap refusals are expected to be rare in practice.

## Location note

`design/classifiers/` as a sibling of `design/query/` is the
recommended-but-not-final home for per-language classifier specs; flagged in
the PR description as a reviewable decision.
