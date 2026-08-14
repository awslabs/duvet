## Unreleased

### Features

* New `duvet query` subcommand for interactive traceability checks during development. Supports five composable checks: `implementation`, `test`, `coverage`, `executed-coverage`, and `duplicates`. Filter results with `--section` and `--quote`; supply coverage data with `--coverage-report` and `--coverage-format`.
* New `duvet-coverage` internal crate providing a Verus-verified two-phase coverage model. Algorithms for scope tree construction, target resolution, and execution-set propagation are formally proven against the correctness properties in `design/query/coverage-model-spec.md`. Used by `duvet query --check coverage` for languages with a tree-sitter classifier; other languages use a verified degraded model that reads coverage directly at the annotation's target line.
* Java line classifier built on tree-sitter; extends the coverage check to handle method declarations, interface bodies, fields without initializers, and other constructs that bytecode-based coverage tools (e.g., JaCoCo) do not report.
* JaCoCo XML coverage report parser.
* The `duplicates` check is now governed by a normative specification (`design/duplicates/spec.md`) and evaluates both coincidence axes in one invocation: claim classes (annotations sharing a section and normalized quote) and target classes (annotations resolving to one source position). Claim-axis rules: two annotations sharing a claim and a resolved target always fail; duplicate sets are bounded by per-form caps (default 1, configurable only for `test` and `implementation`); a claim fully covered by same-form claims outside its own class fails regardless of caps; free claim forms (`exception`, `implication`, `todo`) are exclusive within a claim class by default. The target axis lists fan-in (every target bearing more than one annotation) in every report, and gates it only where `[duplicates.targets]` opts in (`count`, `sections`) or the default form family bites (`test`+`implementation` on one target fails). Policy lives in checked-in `[duplicates.claims]` / `[duplicates.targets]` configuration; the command line never sets policy. `citation` is accepted as a configuration-input alias for `implementation` and is never emitted.
* Free-form exclusivity is a deliberate tightening over the previous `duplicates` check: cross-form coexistence on one claim (e.g. an `exception` and a `test` quoting the same requirement) now fails by default. Prior behavior is restored by one configuration line per axis: `types = ["spec+test+implementation+exception+todo+implication"]` under `[duplicates.claims]` (and under `[duplicates.targets]` for the target axis). Two changes have no restoration knob: same-claim-same-target stacks always fail, and annotations with an empty quote (section-level references) no longer fail the check — the old failure was an accident of an early return, not a rule.
* Upgrade ordering for shared configs: the `[duplicates]` table extends config schema v0.4.0 in place, and older duvet binaries reject configs that use it (`unknown field 'duplicates'`). Upgrade every binary that reads a shared `.duvet/config.toml` before adding a `[duplicates]` section to it.

### Bug Fixes

* Honor `NO_COLOR` and disable ANSI escape codes in diagnostic output when stderr is not a terminal.
* An annotation that references a specification without a `#section` is now reported as an error instead of being silently ignored. Coverage is computed per section, so a section-less reference can never be scored; previously it produced no reference and no diagnostic. If your project relied on the silent behavior, this will surface as new errors in `duvet report`.
* `duvet query --verbose` no longer panics on annotations with a whitespace-only quote.
* Java classifier keeps `Statement` on code lines that carry a trailing `//` comment (e.g. `doX(); // note`), so the coverage check no longer reports such lines as not executed.
* `duvet query --check coverage` builds the scope tree from the pristine classifier output before applying the annotation override, so an annotation trailing a closing brace no longer collapses the file to a single scope.
* The `duplicates` check reports every duplicate relationship instead of hiding exact-duplicate pairs behind a partial-quote coverer.
* The `coverage` check ORs execution status across multiple coverage reports before deciding a correlation, so a test passes when any report proves full coverage (design §5.2).
* The `coverage` check reports tests whose cited specification has no correlated implementation annotation rather than silently passing (design §2.4).

## 0.4.0 (2025-01-22)

### Features

* Added support for configuration files in place of command line arguments ([#152](https://github.com/awslabs/duvet/pull/152))
* New `snapshot` report output, which prevents accidental changes in requirement coverage ([#153](https://github.com/awslabs/duvet/pull/153))
* New `duvet init` command, which creates a configuration file based on the current directory ([#154](https://github.com/awslabs/duvet/pull/154))
* Detailed errors with specific line numbers about what went wrong.

### Bug Fixes

* More robust parsing for both IETF and markdown specification types.

## 0.3.0 (2023-10-06)


### Features

* specify path to the spec files ([#118](https://github.com/awslabs/duvet/issues/118)) ([ce9325e](https://github.com/awslabs/duvet/commit/ce9325ec7e5352f73a26d4b6a4dde34b58b06de1))


## 0.2.0 (2022-11-16)


### Features

* add basic markdown support ([#84](https://github.com/awslabs/duvet/issues/84)) ([f8ebf29](https://github.com/awslabs/duvet/commit/f8ebf298c6dca3c2a261d6a3fbc3703dd1c6703b))


### Bug Fixes

* remove redundant borrows ([#89](https://github.com/awslabs/duvet/issues/89)) ([0cfc8ce](https://github.com/awslabs/duvet/commit/0cfc8ce88a8a5183a68581fd5824498dbe4e376a))
* handle duplicate markdown section names ([#94](https://github.com/awslabs/duvet/issues/94)) ([5d31dd2](https://github.com/awslabs/duvet/commit/5d31dd21c05f5998b8a4e6c66e18552688a3e788))

## 0.1.1 (2022-10-07)

### Features

* Add type implication ([#16](https://github.com/awslabs/duvet/issues/16)) ([45bd9df](https://github.com/awslabs/duvet/commit/45bd9df437ce1788a9b81b6d4d4ff3895b205eec))

### Bug Fixes

* add word boundary assertions for extracted keywords ([#72](https://github.com/awslabs/duvet/issues/72)) ([02c9245](https://github.com/awslabs/duvet/commit/02c92452158debf1be82c702824689ab01b08aa0))
* finish pattern state machine after iterating lines ([#76](https://github.com/awslabs/duvet/issues/76)) ([7d500ff](https://github.com/awslabs/duvet/commit/7d500ffec0bdeaefb1342645965c655b5fd69eed))
* normalize quotes with indentations ([#79](https://github.com/awslabs/duvet/issues/79)) ([65835f7](https://github.com/awslabs/duvet/commit/65835f7cb45c7a84f9f43d7e348225f954a871a5))
* prefix anchors in spec links ([#68](https://github.com/awslabs/duvet/issues/68)) ([93c7875](https://github.com/awslabs/duvet/commit/93c78754f2adb88b4412030b04719c95963f73a1))
* sort Requirements table ([#82](https://github.com/awslabs/duvet/issues/82)) ([71f6152](https://github.com/awslabs/duvet/commit/71f6152dca7a8649823fcddb5a0cccbecc8b7103))
* use BTreeMap for target data ([#86](https://github.com/awslabs/duvet/issues/86)) ([2ea2336](https://github.com/awslabs/duvet/commit/2ea2336fcdd2db247046320c7f3b7b7f4a397bea))
* panic on file without trailing newline ([002fce8](https://github.com/awslabs/duvet/commit/002fce863d7620526e9500d58f9e1268b824841b))
