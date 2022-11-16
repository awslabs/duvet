[//]: # "Copyright Amazon.com Inc. or its affiliates. All Rights Reserved."
[//]: # "SPDX-License-Identifier: CC-BY-SA-4.0"

# Encrypt

## Version

0.4.0

### Changelog

- 0.3.0

  - [Clarify Streaming Encrypt and Decrypt](../changes/2020-07-06_clarify-streaming-encrypt-decrypt/change.md)

- 0.2.0

  - [Remove Keyring Trace](../changes/2020-05-13_remove-keyring-trace/change.md)

- 0.1.0-preview

  - Initial record

## Implementations

| Language   | Confirmed Compatible with Spec Version | Minimum Version Confirmed | Implementation                                                                                                                                                 |
| ---------- | -------------------------------------- | ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| C          | 0.1.0-preview                          | 0.1.0                     | [session_encrypt.c](https://github.com/aws/aws-encryption-sdk-c/blob/master/source/session_encrypt.c)                                                          |
| NodeJS     | 0.1.0-preview                          | 0.1.0                     | [encrypt.ts](https://github.com/awslabs/aws-encryption-sdk-javascript/blob/master/modules/encrypt-node/src/encrypt.ts)                                         |
| Browser JS | 0.1.0-preview                          | 0.1.0                     | [encrypt.ts](https://github.com/awslabs/aws-encryption-sdk-javascript/blob/master/modules/encrypt-browser/src/encrypt.ts)                                      |
| Python     | 0.1.0-preview                          | 1.2.0                     | [streaming_client.py](https://github.com/aws/aws-encryption-sdk-python/blob/master/src/aws_encryption_sdk/streaming_client.py)                                 |
| Java       | 0.1.0-preview                          | 0.0.1                     | [EncryptionHandler.java](https://github.com/aws/aws-encryption-sdk-java/blob/master/src/main/java/com/amazonaws/encryptionsdk/internal/EncryptionHandler.java) |

## Overview

This document describes the behavior by which a plaintext is encrypted and serialized into a [message](../data-format/message.md).

Any client provided by the AWS Encryption SDK that performs encryption of caller plaintext MUST follow
this specification for encryption.

## Input

The following inputs to this behavior are REQUIRED:

- [Plaintext ](#plaintext)
- Either a [Cryptographic Materials Manager (CMM)](../framework/cmm-interface.md) or a [Keyring](../framework/keyring-interface.md)

The following inputs to this behavior MUST be OPTIONAL:

- [Algorithm Suite](#algorithm-suite)
- [Encryption Context](#encryption-context)
- [Frame Length](#frame-length)

If the [plaintext ](#plaintext) is of unknown length, the caller MAY also input a
[Plaintext Length Bound](#plaintext-length-bound).

Implementations SHOULD ensure that a caller is not able to specify both a [plaintext ](#plaintext)
with known length and a [Plaintext Length Bound](#plaintext-length-bound) by construction.
If a caller is able to specify both an input [plaintext ](#plaintext) with known length and
a [Plaintext Length Bound](#plaintext-length-bound),
the [Plaintext Length Bound](#plaintext-length-bound) MUST NOT be used during the Encrypt operation
and MUST be ignored.

### Plaintext

The plaintext to encrypt.
This MUST be a sequence of bytes.

This input MAY be [streamed](streaming.md) to this operation.

If an implementation requires holding the input entire plaintext in memory in order to perform this operation,
that implementation SHOULD NOT provide an API that allows this input to be streamed.

### Encryption Context

See [encryption context](../framework/structures.md#encryption-context).

The prefix `aws-crypto-` is reserved for internal use by the AWS Encryption SDK;
see the [the Default CMM spec](default-cmm.md) for one such use.
If the input encryption context contains any entries with a key beginning with this prefix,
the encryption operation MUST fail.

### CMM

A CMM that implements the [CMM interface](../framework/cmm-interface.md).

### Keyring

A Keyring that implements the [keyring interface](../framework/keyring-interface.md).

### Algorithm Suite

The [algorithm suite](../framework/algorithm-suites.md) that SHOULD be used for encryption.

### Frame Length

The [frame length](../data-format/message-header.md#frame-length) to use for [framed data](../data-format/message-body.md).
This value MUST be greater than 0 and MUST NOT exceed the value 2^32 - 1.
This value MUST default to 4096 bytes.

### Plaintext Length Bound

A bound on the length of [plaintext ](#plaintext) with an unknown length to encrypt.

If this input is provided, this operation MUST NOT encrypt a plaintext with length
greater than this value.

## Output

This behavior MUST output the following if the behavior is successful:

- [Encrypted Message](#encrypted-message)
- [Encryption Context](#encryption-context)
- [Algorithm Suite](#algorithm-suite)

The client SHOULD return as an output:

- [Parsed Header](#parsed-header)

### Encrypted Message

An encrypted form of the input [plaintext ](#plaintext),
encrypted according to the [behavior specified below](#behavior).
This MUST be a sequence of bytes
and conform to the [message format specification](../data-format/message.md).

This operation MAY [stream](streaming.md) the encrypted message.

If an implementation requires holding the entire input plaintext in memory in order to perform this operation,
that implementation SHOULD NOT provide an API that allows this output to be streamed.

### Encryption Context

The [encryption context](../framework/structures.md#encryption-context) that is used as
additional authenticated data during the encryption of the input [plaintext ](#plaintext).

This output MAY be satisfied by outputting a [parsed header](#parsed-header) containing this value.

### Algorithm Suite

The [algorithm suite](../framework/algorithm-suites.md) that is used to encrypt
the input [plaintext ](#plaintext).

This output MAY be satisfied by outputting a [parsed header](#parsed-header) containing this value.

### Parsed Header

A collection of deserialized fields of the [encrypted message's](#encrypted-message) header.

## Behavior

The Encrypt operation is divided into several distinct steps:

- [Get the encryption materials](#get-the-encryption-materials)
- [Construct the header](#construct-the-header)
- [Construct the body](#construct-the-body)
- [Construct the signature](#construct-the-signature)
  - If the [encryption materials gathered](#get-the-encryption-materials) has a algorithm suite
    including a [signature algorithm](../framework/algorithm-suites.md#signature-algorithm),
    the encrypt operation MUST perform this step.
    Otherwise the encrypt operation MUST NOT perform this step.

This operation MUST perform all the above steps unless otherwise specified,
and it MUST perform them in the above order.

These steps calculate and serialize the components of the output [encrypted message](#encrypted-message).
Any data that is not specified within the [message format](../data-format/message.md)
MUST NOT be added to the output message.

If any of these steps fails, this operation MUST halt and indicate a failure to the caller.

### Get the encryption materials

If an [input algorithm suite](#algorithm-suite) is provided
that is not supported by the [commitment policy](client.md#commitment-policy)
configured in the [client](client.md) encrypt MUST yield an error.

To construct the [encrypted message](#encrypted-message),
some fields MUST be constructed using information obtained
from a set of valid [encryption materials](../framework/structures.md#encryption-materials).
This operation MUST obtain this set of [encryption materials](../framework/structures.md#encryption-materials)
by calling [Get Encryption Materials](../framework/cmm-interface.md#get-encryption-materials) on a [CMM](../framework/cmm-interface.md).
The CMM used MUST be the input CMM, if supplied.
If instead the caller supplied a [keyring](../framework/keyring-interface.md),
this behavior MUST use a [default CMM](../framework/default-cmm.md)
constructed using the caller-supplied keyring as input.
The call to [Get Encryption Materials](../framework/cmm-interface.md#get-encryption-materials)
on that CMM MUST be constructed as follows:

- Encryption Context: If provided, this is the [input encryption context](#encryption-context).
  Otherwise, this is an empty encryption context.
- Commitment Policy: The [commitment policy](client.md#commitment-policy) configured in the [client](client.md) exposing this encrypt function.
- Algorithm Suite: If provided, this is the [input algorithm suite](#algorithm-suite).
  Otherwise, this field is not included.
- Max Plaintext Length: If the [input plaintext](#plaintext) has known length,
  this length MUST be used.
  If the input [plaintext ](#plaintext) has unknown length and a [Plaintext Length Bound](#plaintext-length-bound)
  was provided, this is the [Plaintext Length Bound](#plaintext-length-bound).
  Otherwise, this field is not included.

The [algorithm suite](../framework/algorithm-suites.md) used in all aspects of this operation
MUST be the algorithm suite in the [encryption materials](../framework/structures.md#encryption-materials)
returned from the [Get Encryption Materials](../framework/cmm-interface.md#get-encryption-materials) call.
Note that the algorithm suite in the retrieved encryption materials MAY be different
from the [input algorithm suite](#algorithm-suite).
If this [algorithm suite](../framework/algorithm-suites.md) is not supported by the [commitment policy](client.md#commitment-policy)
configured in the [client](client.md) encrypt MUST yield an error.
If the number of [encrypted data keys](../framework/structures.md#encrypted-data-keys) on the [encryption materials](../framework/structures.md#encryption-materials)
is greater than the [maximum number of encrypted data keys](client.md#maximum-number-of-encrypted-data-keys) configured in the [client](client.md) encrypt MUST yield an error.

The data key used as input for all encryption described below is a data key derived from the plaintext data key
included in the [encryption materials](../framework/structures.md#encryption-materials).
The algorithm used to derive a data key from the plaintext data key MUST be
the [key derivation algorithm](../framework/algorithm-suites.md#key-derivation-algorithm) included in the
[algorithm suite](../framework/algorithm-suites.md) defined above.
This document refers to the output of the key derivation algorithm as the derived data key.
Note that if the key derivation algorithm is the [identity KDF](../framework/algorithm-suites.md#identity-kdf),
then the derived data key is the same as the plaintext data key.

The frame length used in the procedures described below is the input [frame length](#frame-length),
if supplied, or the default if not.

### Construct the header

Before encrypting input plaintext,
this operation MUST serialize the [message header body](../data-format/message-header.md).
The [message format version](../data-format/message-header.md#supported-versions) MUST be associated with the [algorithm suite](../framework/algorithm-suites.md#supported-algorithm-suites).

If the message format version associated with the [algorithm suite](../framework/algorithm-suites.md#supported-algorithm-suites) is 2.0
then the [message header body](../data-format/message-header.md#header-body-version-1-0) MUST be serialized with the following specifics:

- [Version](../data-format/message-header.md#version-1): MUST have a value corresponding to
  [2.0](../data-format/message-header.md#supported-versions)
- [Algorithm Suite ID](../data-format/message-header.md#algorithm-suite-id): MUST correspond to
  the [algorithm suite](../framework/algorithm-suites.md) used in this behavior
- [Message ID](../data-format/message-header.md#message-id): The process used to generate
  this identifier MUST use a good source of randomness to make the chance of duplicate identifiers negligible.
- [AAD](../data-format/message-header.md#aad): MUST be the serialization of the [encryption context](../framework/structures.md#encryption-context)
  in the [encryption materials](../framework/structures.md#encryption-materials)
- [Encrypted Data Keys](../data-format/message-header.md#encrypted-data-key-entries): MUST be the serialization of the
  [encrypted data keys](../framework/structures.md#encrypted-data-keys) in the [encryption materials](../framework/structures.md#encryption-materials)
- [Content Type](../data-format/message-header.md#content-type): MUST be [02](../data-format/message-header.md#supported-content-types)
- [Frame Length](../data-format/message-header.md#frame-length): MUST be the value of the frame size determined above.
- [Algorithm Suite Data](../data-format/message-header.md#algorithm-suite-data): MUST be the value of the [commit key](../framework/algorithm-suites.md#commit-key)
  derived according to the [algorithm suites commit key derivation settings](../framework/algorithm-suites.md#algorithm-suites-commit-key-derivation-settings).

If the message format version associated with the [algorithm suite](../framework/algorithm-suites.md#supported-algorithm-suites) is 1.0
then the [message header body](../data-format/message-header.md#header-body-version-1-0) MUST be serialized with the following specifics:

- [Version](../data-format/message-header.md#version-1): MUST have a value corresponding to
  [1.0](../data-format/message-header.md#supported-versions)
- [Type](../data-format/message-header.md#type): MUST have a value corresponding to
  [Customer Authenticated Encrypted Data](../data-format/message-header.md#supported-types)
- [Algorithm Suite ID](../data-format/message-header.md#algorithm-suite-id): MUST correspond to
  the [algorithm suite](../framework/algorithm-suites.md) used in this behavior
- [Message ID](../data-format/message-header.md#message-id): The process used to generate
  this identifier MUST use a good source of randomness to make the chance of duplicate identifiers negligible.
- [AAD](../data-format/message-header.md#aad): MUST be the serialization of the [encryption context](../framework/structures.md#encryption-context)
  in the [encryption materials](../framework/structures.md#encryption-materials)
- [Encrypted Data Keys](../data-format/message-header.md#encrypted-data-key-entries): MUST be the serialization of the
  [encrypted data keys](../framework/structures.md#encrypted-data-keys) in the [encryption materials](../framework/structures.md#encryption-materials)
- [Content Type](../data-format/message-header.md#content-type): MUST be [02](../data-format/message-header.md#supported-content-types)
- [IV Length](../data-format/message-header.md#iv-length): MUST match the [IV length](../framework/algorithm-suites.md#iv-length)
  specified by the [algorithm suite](../framework/algorithm-suites.md)
- [Frame Length](../data-format/message-header.md#frame-length): MUST be the value of the frame size determined above.

After serializing the message header body,
this operation MUST calculate an [authentication tag](../data-format/message-header.md#authentication-tag)
over the message header body.
The value of this MUST be the output of the [authenticated encryption algorithm](../framework/algorithm-suites.md#encryption-algorithm)
specified by the [algorithm suite](../framework/algorithm-suites.md), with the following inputs:

- The AAD is the serialized [message header body](../data-format/message-header.md#header-body).
- The IV has a value of 0.
- The cipherkey is the derived data key
- The plaintext is an empty byte array

With the authentication tag calculated,
if the message format version associated with the [algorithm suite](../framework/algorithm-suites.md#supported-algorithm-suites) is 2.0,
this operation MUST serialize the [message header authentication](../data-format/message-header.md#header-authentication-version-2-0) with the following specifics:

- [Authentication Tag](../data-format/message-header.md#authentication-tag): MUST have the value
  of the authentication tag calculated above.

If the message format version associated with the [algorithm suite](../framework/algorithm-suites.md#supported-algorithm-suites) is 1.0
this operation MUST serialize the [message header authentication](../data-format/message-header.md#header-authentication-version-1-0) with the following specifics:

- [IV](../data-format/message-header.md#iv): MUST have the value of the IV used in the calculation above,
  padded to the [IV length](../data-format/message-header.md#iv-length) with 0.
- [Authentication Tag](../data-format/message-header.md#authentication-tag): MUST have the value
  of the authentication tag calculated above.

The serialized bytes MUST NOT be released until the entire message header has been serialized
If this operation is streaming the encrypted message and
the entire message header has been serialized,
the serialized message header SHOULD be released.

The encrypted message output by this operation MUST have a message header equal
to the message header calculated in this step.

If the algorithm suite contains a signature algorithm and
this operation is [streaming](streaming.md) the encrypted message output to the caller,
this operation MUST input the serialized header to the signature algorithm as soon as it is serialized,
such that the serialized header isn't required to remain in memory to [construct the signature](#construct-the-signature).

## Construct the body

The encrypted message output by this operation MUST have a message body equal
to the message body calculated in this step.

If [Plaintext Length Bound](#plaintext-length-bound) was specified on input
and this operation determines at any time that the plaintext being encrypted
has a length greater than this value,
this operation MUST immediately fail.

Before the end of the input is indicated,
this operation MUST process as much of the consumable bytes as possible
by [constructing regular frames](#construct-a-frame).

When the end of the input is indicated,
this operation MUST perform the following until all consumable plaintext bytes are processed:

- If there are exactly enough consumable plaintext bytes to create one regular frame,
  such that creating a regular frame processes all consumable bytes,
  then this operation MUST [construct either a final frame or regular frame](#construct-a-frame)
  with the remaining plaintext.
- If there are enough input plaintext bytes consumable to create a new regular frame,
  such that creating a regular frame does not processes all consumable bytes,
  then this operation MUST [construct a regular frame](#construct-a-frame)
  using the consumable plaintext bytes.
- If there are not enough input consumable plaintext bytes to create a new regular frame,
  then this operation MUST [construct a final frame](#construct-a-frame)

If an end to the input has been indicated, there are no more consumable plaintext bytes to process,
and a final frame has not yet been constructed,
this operation MUST [construct an empty final frame](#construct-a-frame).

### Construct a frame

To construct a regular or final frame that represents the next frame in the encrypted message's body,
this operation MUST calculate the encrypted content and an authentication tag using the
[authenticated encryption algorithm](../framework/algorithm-suites.md#encryption-algorithm)
specified by the [algorithm suite](../framework/algorithm-suites.md),
with the following inputs:

- The AAD is the serialized [message body AAD](../data-format/message-body-aad.md),
  constructed as follows:
  - The [message ID](../data-format/message-body-aad.md#message-id) is the same as the
    [message ID](../data-frame/message-header.md#message-id) serialized in the header of this message.
  - The [Body AAD Content](../data-format/message-body-aad.md#body-aad-content) depends on
    whether the thing being encrypted is a regular frame or final frame.
    Refer to [Message Body AAD](../data-format/message-body-aad.md) specification for more information.
  - The [sequence number](../data-format/message-body-aad.md#sequence-number) is the sequence
    number of the frame being encrypted.
    If this is the first frame sequentially, this value MUST be 1.
    Otherwise, this value MUST be 1 greater than the value of the sequence number
    of the previous frame.
  - The [content length](../data-format/message-body-aad.md#content-length) MUST have a value
    equal to the length of the plaintext being encrypted.
    - For a regular frame the length of this plaintext MUST equal the frame length.
    - For a final frame this MUST be the length of the remaining plaintext bytes
      which have not yet been encrypted,
      whose length MUST be equal to or less than the frame length.
- The IV is the [sequence number](../data-format/message-body-aad.md#sequence-number)
  used in the message body AAD above,
  padded to the [IV length](../data-format/message-header.md#iv-length).
- The cipherkey is the derived data key
- The plaintext is the next subsequence of consumable plaintext bytes that have not yet been encrypted.
  - For a regular frame the length of this plaintext subsequence MUST equal the frame length.
  - For a final frame this MUST be the remaining plaintext bytes which have not yet been encrypted,
    whose length MUST be equal to or less than the frame length.

This operation MUST serialize a regular frame or final frame with the following specifics:

- [Sequence Number](../data-format/message-body.md#sequence-number): MUST be the sequence number of this frame,
  as determined above.
- [IV](../data-format/message-body.md#iv): MUST be the IV used when calculating the encrypted content above
- [Encrypted Content](../data-format/message-body.md#encrypted-content): MUST be the encrypted content calculated above.
- [Authentication Tag](../data-format/message-body.md#authentication-tag): MUST be the authentication tag
  output when calculating the encrypted content above.

The above serialized bytes MUST NOT be released until the entire frame has been serialized.
If this operation is streaming the encrypted message and
the entire frame has been serialized,
the serialized frame SHOULD be released.

If the algorithm suite contains a signature algorithm and
this operation is [streaming](streaming.md) the encrypted message output to the caller,
this operation MUST input the serialized frame to the signature algorithm as soon as it is serialized,
such that the serialized frame isn't required to remain in memory to [construct the signature](#construct-the-signature).

### Construct the signature

If the [algorithm suite](../framework/algorithm-suites.md) contains a [signature algorithm](../framework/algorithm-suites.md#signature-algorithm),
this operation MUST calculate a signature over the message,
and the output [encrypted message](#encrypted-message) MUST contain a [message footer](../data-format/message-footer.md).

To calculate a signature, this operation MUST use the [signature algorithm](../framework/algorithm-suites.md#signature-algorithm)
specified by the [algorithm suite](../framework/algorithm-suites.md), with the following input:

- the signature key is the [signing key](../framework/structures.md#signing-key) in the [encryption materials](../framework/structures.md#encryption-materials)
- the input to sign is the concatenation of the serialization of the [message header](../data-format/message-header.md) and [message body](../data-format/message-body.md)

Note that the message header and message body MAY have already been input during previous steps.

This operation MUST then serialize a message footer with the following specifics:

- [Signature Length](../data-format/message-footer.md#signature-length): MUST be the length of the
  output of the calculation above.
- [Signature](../data-format/message-footer.md#signature): MUST be the output of the calculation above.

The above serialized bytes MUST NOT be released until the entire message footer has been serialized.
Once the entire message footer has been serialized,
this operation MUST release any previously unreleased serialized bytes from previous steps
and MUST release the message footer.

The encrypted message output by this operation MUST have a message footer equal
to the message footer calculated in this step.

## Appendix

### Un-Framed Message Body Encryption

Implementations of the AWS Encryption SDK MUST NOT encrypt using the Non-Framed content type.
However, this behavior was supported in the past.

If a message has the [non-framed](../data-format/message-body.md#non-framed-data) content type,
the [message body](../data-format/message-body.md) was serialized with the following specifics:

- [IV](../data-format/message-body.md#iv): MUST be the [sequence number](../data-format/message-body-aad.md#sequence-number)
  used in the [message body AAD](../data-format/message-body-aad.md).
- [Encrypted Content](../data-format/message-body.md#encrypted-content): MUST be the output of the [authenticated encryption algorithm](../framework/algorithm-suites.md#encryption-algorithm)
  specified by the [algorithm suite](../framework/algorithm-suites.md), with the following inputs:
  - The AAD is the serialized [message body AAD](../data-format/message-body-aad.md)
  - The IV is the [IV](../data-format/message-body.md#iv) specified above.
  - The cipherkey is the derived data key
  - The plaintext is the input [plaintext ](#plaintext)
- [Authentication Tag](../data-format/message-body.md#authentication-tag): MUST be the authentication tag returned by the above encryption.
