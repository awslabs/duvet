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

## Development

### Building

```console
$ cargo xtask build
```

### Testing

```console
$ cargo xtask test
```

## Security

See [CONTRIBUTING](CONTRIBUTING.md#security-issue-notifications) for more information.

## License

This project is licensed under the Apache-2.0 License.
