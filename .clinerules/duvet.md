# Duvet

# Introduction

Duvet is a tool that establishes a bidirectional link between implementation and specification. This practice is called [requirements traceability](https://en.wikipedia.org/wiki/Requirements_traceability), which is defined as:

> the ability to describe and follow the life of a requirement in both a forwards and backwards direction (i.e., from its origins, through its development and specification, to its subsequent deployment and use, and through periods of ongoing refinement and iteration in any of these phases)

## Annotations

Duvet scans source code for special comments containing references to specification text. By default, the comment style is the following:

```rust
//= https://www.rfc-editor.org/rfc/rfc2324#section-2.1.1
//# A coffee pot server MUST accept both the BREW and POST method
//# equivalently.
```

If the default comment style is not compatible with the language being used, it can be changed in the [configuration](./config.md) with the `comment-style` field.

The default type of annotation is `implementation`, meaning the reference is implementing the cited text. The type of annotation can be changed with the `type` parameter. Duvet supports the following annotation types:

### `implementation`

The source code is aiming to implement the cited text from the specification. This is the default annotation type.

### `test`

The source code is aiming to test that the program implements the cited text correctly.

```rust
//= https://www.rfc-editor.org/rfc/rfc2324#section-2.1.1
//= type=test
//# A coffee pot server MUST accept both the BREW and POST method
//# equivalently.
#[test]
fn my_test() {
    // TODO
}
```

### `implication`

The source code is both implementing and testing the cited text. This can be useful for requirements that are correct by construction. For example, let's say our specification says the following:

```
# Section

The function MUST return a 64-bit integer.
```

In a strongly-typed language, this requirement is being both implemented and tested by the compiler.

```rust
//= my-spec.md#section
//= type=implication
//# The function MUST return a 64-bit integer.
fn the_function() -> u64 {
    42
}
```

### `exception`

The source code has defined an exception for a requirement and is explicitly choosing not to implement it. This could be for various reasons. For example, let's consider the following specification:

```
# Section

Implementations MAY panic on invalid arguments.
```

In our example here, we've chosen _not_ to panic, but instead return an error. Annotations with the `exception` type can optionally provide a reason as to why the requirement is not being implemented.

```rust
//= my-spec.md#section
//= type=exception
//= reason=We prefer to return errors that can be handled by the caller.
//# Implementations MAY panic on invalid arguments.
fn the_function() -> Result<u64, Error> {
    // implementation here
}
```

### `todo`

Some requirements may not be currently implemented but are on the product's roadmap. Such requirements can be annotated with the `todo` type to indicate this. Optionally, the annotation can provide a tracking issue for more context/updates.

```rust
//= my-spec.md#section
//= type=todo
//= tracking-issue=1234
//# Implementations SHOULD do this thing.
```

### `spec`

The `spec` annotation type provides a way to annotate additional text in a specification that does not use the key words from [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119), but is still considered as providing a requirement.

```
# Section

It's really important that implementations validate untrusted input.
```

```rust
//= my-spec.md#section
//= type=spec
//= level=MUST
//# It's really important that implementations validate untrusted input.
```

Additionally, Duvet also supports defining these requirements in `toml`:

```toml
[[spec]]
target = "my-spec.md#section"
level = "MUST"
quote = '''
It's really important that implementations validate untrusted input.
'''
```

## Reports

Duvet provides a `duvet report` command to provide insight into requirement coverage for a project. Each report has its own [configuration](./config.md).

### HTML

The `html` report is enabled by default. It's rendered in a browser and makes it easy to explore all of the specifications being annotated and provides statuses for each requirement. Additionally, the specifications are highlighted with links back to the project's source code, which establishes a bidirectional link between source and specification.

### Snapshot

The `snapshot` report provides a mechanism for projects to ensure requirement coverage does not change without explicit approvals. It accomplishes this by writing a simple text file to `.duvet/snapshot.txt` that can detect differences in requirements coverage. It can also track progress of how many requirements are complete and still remaining.

This is what is known as a "snapshot test". Note that in order for this to work, the `snapshot.txt` file needs to be checked in to the source code's version control system, which ensures that it always tracks the state of the code.

If you are unsure if a citation is correct, running `duvet report` will return any errors that exist.
