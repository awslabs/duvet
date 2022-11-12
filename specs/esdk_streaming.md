[//]: # "Copyright Amazon.com Inc. or its affiliates. All Rights Reserved."
[//]: # "SPDX-License-Identifier: CC-BY-SA-4.0"

# Streaming

## Version

0.1.0

### Changelog

- 0.1.0

  - [Clarify Streaming Encrypt and Decrypt](../changes/2020-07-06_clarify-streaming-encrypt-decrypt/change.md)

## Overview

The AWS Encryption SDK MAY provide APIs that enable streamed [encryption](encrypt.md)
and [decryption](decrypt.md).
Streaming is a framework for making bytes available to be processed
by an operation sequentially and over time,
and for outputting the result of that processing
sequentially and over time.

If an implementation requires holding the entire input in memory in order to perform the operation,
that implementation SHOULD NOT provide an API that allows the caller to stream the operation.
APIs that support streaming of the encrypt or decrypt operation SHOULD allow customers
to be able to process arbitrarily large inputs with a finite amount of working memory.

## Definitions

### Conventions used in this document

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL"
in this document are to be interpreted as described in [RFC2119](https://tools.ietf.org/html/rfc2119).

### Consumable Bytes

In the scope of an operation, bytes are considered consumable if:

- The operation has not yet processed those bytes.
- The operation has access to those bytes.
- Those bytes are intended to be processed.
  This intention is expressed through the specific streaming interface.

For example, in a framework where a customer is sending input bytes to an operation
and that operation must write the output bytes to some sink,
the input bytes received from the customer are considered consumable.
Here the customer is expressing intent to process their supplied bytes.

For a framework where a customer is requesting output bytes from an operation
and that operation must read from some source in order to produce bytes,
this is slightly more complicated.
Bytes are considered consumable if:

- Those bytes have not yet been processed.
- Those bytes are able to be read by the operation from the source.
- Those bytes are required to be processed in order for the operation
  to release the output requested by the customer.
  Here the customer expresses intent for the operation to process
  whatever the operation needs to consume to produce its complete output

### Release

An operation releases bytes when the operation intends those bytes to be considered output.

For example, in a framework where a customer is sending input bytes to an operation
and that operation must write the output bytes to some sink,
bytes are considered released once the operation writes those bytes into the sink.

For a framework where a customer is requesting output bytes from an operation
and that operation must read from some source in order to produce bytes,
bytes are considered released once those bytes are available to be read by the customer.

If bytes are processed by an operation, that does not imply that the operation is allowed to
release any result of that processing.
The decrypt and encrypt operations specify when output bytes MUST NOT be released
and when they SHOULD be released.

## Inputs

In order to support streaming, the operation MUST accept some input within a streaming framework.

This means that:

- There MUST be a mechanism for input bytes to become consumable.
- There MUST be a mechanism to indicate that there are no more input bytes.

These mechanisms are used to allow the operation to process input bytes in parts, over time.

The bytes that represent the entire input to the operation are the bytes that the customer intended
to be processed.

## Outputs

In order to support streaming, the operation MUST produce some output within a streaming framework.

This means that:

- There MUST be a mechanism for output bytes to be released.
- There MUST be a mechanism to indicate that the entire output has been released.

These mechanisms are used to allow the operation to produce output bytes in parts, over time.

The bytes that represent the entire output to the operation are the bytes that were released
up until an end was indicated.

Operations MUST NOT indicate completion or success until an end to the output has been indicated.

## Behavior

By using the mechanisms for [inputs](#inputs) and [outputs](#outputs),
some actor expresses intent through a streaming interface
for bytes to be made consumable to the operation
and for bytes to be released by the operation.

The behavior of the operation specifies how the operation processes consumable bytes,
and specifies when processed bytes MUST NOT and SHOULD be released.
