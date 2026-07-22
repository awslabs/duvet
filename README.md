# Duvet

Duvet is a tool that establishes a bidirectional link between implementation and specification. This practice is called [requirements traceability](https://en.wikipedia.org/wiki/Requirements_traceability), which is defined as:

> the ability to describe and follow the life of a requirement in both a forwards and backwards direction (i.e., from its origins, through its development and specification, to its subsequent deployment and use, and through periods of ongoing refinement and iteration in any of these phases)

## Quick Start

Before getting started, Duvet requires a [rust toolchain](https://www.rust-lang.org/tools/install).

1. Install command

    ```console
    $ cargo install duvet --locked
    ```

2. Initialize repository

    In this example, we are using Rust. However, Duvet can be used with any language.

    ```console
    $ duvet init --lang-rust --specification https://www.rfc-editor.org/rfc/rfc2324
    ```

3. Add a implementation comment in the project

    ```rust
    // src/lib.rs

    //= https://www.rfc-editor.org/rfc/rfc2324#section-2.1.1
    //# A coffee pot server MUST accept both the BREW and POST method
    //# equivalently.
    ```

4. Generate a report

    ```console
    $ duvet report
    ```

## Querying traceability state

`duvet query` is a development-time companion to `duvet report`. Where `report` produces the full traceability artifact for CI, `query` answers focused questions during development:

- *Have I annotated the requirements I'm working on?*
- *Does my implementation have a test?*
- *Did the test actually execute the implementation?*
- *Are there duplicate annotations covering the same text?*

Each question is a check (`--check` / `-c`), composable per invocation:

```console
$ duvet query -c implementation,test --section my-spec.md
$ duvet query -c coverage -r 'target/jacoco/*.xml' -f jacoco-xml
```

The coverage check correlates test annotations with executed implementation annotations using a coverage report. For Java sources, duvet uses a verified two-phase coverage model that understands method declarations and other constructs that don't appear in bytecode-based coverage data; for other languages it uses a verified degraded model that reads coverage directly at the annotation's target line.

See the [guide](./guide/src/query.md) for the full reference.

## Development

You must have `git lfs` installed. You can check this by running
```shell
git lfs version
```

### Building

```console
$ cargo xtask build
```

### Testing

```console
$ cargo xtask test
```

### Verifying the coverage model

The two-phase coverage model in `duvet-coverage` is formally verified
with [Verus](https://verus-lang.github.io/verus/guide/). CI runs the
verifier on every push and PR. To verify locally:

1. **Download the pinned Verus release.** The version that matches
   `vstd` in `Cargo.lock` is in `.github/workflows/ci.yml` as
   `VERUS_VERSION`. Verus ships pre-built binaries for x86_64 Linux,
   x86_64 macOS, arm64 macOS, and x86_64 Windows; pick the one for
   your host:

   ```console
   $ VERUS_VERSION="0.2026.05.24.ecee80a"
   $ curl -L -o verus.zip \
       "https://github.com/verus-lang/verus/releases/download/release%2F${VERUS_VERSION}/verus-${VERUS_VERSION}-x86-linux.zip"
   $ unzip -q verus.zip
   ```

   The archive extracts to `verus-x86-linux/` (or `-x86-macos`, etc.)
   and contains the `verus` and `cargo-verus` binaries, the bundled
   `z3` solver, and the `vstd` standard library.

2. **Add the directory to `PATH`.**

   ```console
   $ export PATH="$PWD/verus-x86-linux:$PATH"
   ```

3. **Install the Rust toolchain Verus needs.** The first time `verus`
   runs, it prints the exact `rustup install` command for the
   toolchain it pins. Run that command:

   ```console
   $ verus
   verus: required rust toolchain X.Y.Z-x86_64-unknown-linux-gnu not found
   run the following command (in a bash-compatible shell) to install the necessary toolchain:
     rustup install X.Y.Z-x86_64-unknown-linux-gnu
   ...
   $ rustup install X.Y.Z-x86_64-unknown-linux-gnu
   ```

   On macOS, you may also need to clear the Gatekeeper quarantine on
   the binaries; the archive includes `macos_allow_gatekeeper.sh`.

4. **Verify the proofs.**

   ```console
   $ cargo verus build -p duvet-coverage
   ```

   Expected output: `verified N functions, 0 errors`. A non-zero error
   count indicates a regression in the proofs.

The Verus prebuilt binary (and the `z3` it bundles) are built against
glibc 2.34+ / 2.31+, so older distributions (Amazon Linux 2, Ubuntu
20.04, CentOS 7) cannot run them. On those hosts, build Verus — and, if
its prebuilt `z3` also won't start, z3 — from source so they link against
the local glibc (see Verus's `BUILD.md`: `vargo build --release`, then put
`source/target-verus/release` on `PATH`).

A from-source `z3` reports a build-hash-suffixed SMT version string that
the verifier rejects by default. It is still the pinned z3 version, so
results are identical; pass the (supported) flag to skip the cosmetic
check — `vargo build` accepts `--no-solver-version-check`, and the verify
step uses the `-V` form:

```console
$ cargo verus build -p duvet-coverage -- -V no-solver-version-check
```

Otherwise, rely on CI for verification.

## Security

See [CONTRIBUTING](CONTRIBUTING.md#security-issue-notifications) for more information.

## License

This project is licensed under the Apache-2.0 License.
