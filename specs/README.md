[//]: # "Copyright Amazon.com Inc. or its affiliates. All Rights Reserved."
[//]: # "SPDX-License-Identifier: CC-BY-SA-4.0"

# Duvet Specification

## Overview

This directory contains the specification for Duvet.
The primary goal of this specification is to define a standard,
language independent, description of the Duvet features.
It serves as the source of truth for the features that make up Duvet
and the details of their behavior.

## Editing

We use `prettier` to maintain consistent formatting.
Our CI will stop PRs that do not match our formatting requirements,
but to easily apply them,
run `./ci/prettify.sh write`.
If you want to check them without writing,
run `./ci/prettify.sh check`.

We prefer authors adhere to [semantic line breaks](https://sembr.org/),
but we do not enforce it.

## Proposals and Changes

Proposals for new features or changes to the current features of Duvet should be
authored in the proposals' directory, following the format used by the
[AWS Encryption SDK Specification](https://github.com/awslabs/aws-encryption-sdk-specification/blob/a2ba123eb42b863bba1babf412af374018b35c0c/proposals/2020-06-26_decrypt-max-header-size-max-body-size/proposal.md).

Accepted proposals will change the Duvet Specification,
but should still be documented as a change as is done by the
[AWS Encryption SDK Specification](https://github.com/awslabs/aws-encryption-sdk-specification/tree/master/changes).

## Exporting to RFC and TOML

### Extract `compliance` from Specification

The Specification is written in Markdown.
Currently, Duvet needs RFC formatted text and TOML files.
As such, we have a tool that extracts the RFC spec
and supporting TOML files from the Markdown.

### Running `extract`

The entire specification may be extracted at once. Run:

```
./util/specification_extract.sh
```

### Installing dependencies

The utility/script `util/specification_extract.sh` depends on four run
times: `node`, `python`, `ruby`, and `rust`.
(No, this is not ideal, but
Duvet is pushing the "spec to code" boundary;
we are ahead of the tooling.)

#### Set Up Python & `xml2rfc`

Follow
[AWS Crypto Tools Getting Started with Python instructions](https://github.com/aws/crypto-tools/blob/master/getting-started/python/README.md#local-development-setup)
to install `pyenv`.

Then, in this repository, run
`pyenv local 3.9.7; pyenv exec python -m pip install xml2rfc==3.5.0 markupsafe==2.0.1`.

#### Set up `kramdown-rfc2629`

This is the Ruby dependency. Unfortunately, we have not figured out
a good way of installing this, so we do a bad way:

```
sudo gem install kramdown-rfc2629
```

#### Node

Follow
[Installing Node.js with `nvm` macOS by Daniel Schildt](https://gist.github.com/d2s/372b5943bce17b964a79)
to get `nvm` and `node` working.

#### Rust

Installing Duvet will install rust.
