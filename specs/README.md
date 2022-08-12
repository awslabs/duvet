[//]: # "Copyright Amazon.com Inc. or its affiliates. All Rights Reserved."
[//]: # "SPDX-License-Identifier: CC-BY-SA-4.0"

# Duvet Specification

## Overview

This directory contains the specification for Duvet.
The primary goal of this specification is to define a standard,
language independent, description of the Duvet features.
t serves as the source of truth for the features that make up Duvet
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
