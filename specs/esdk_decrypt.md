[//]: # "Copyright Amazon.com Inc. or its affiliates. All Rights Reserved."
[//]: # "SPDX-License-Identifier: CC-BY-SA-4.0"

# Decrypt

## Version

0.4.0

### Changelog

- 0.4.0

  - Add unsigned streaming decryption option

- 0.3.0

  - [Clarify Streaming Encrypt and Decrypt](../changes/2020-07-06_clarify-streaming-encrypt-decrypt/change.md)

- 0.2.0

  - [Detect Base64-encoded Messages](../changes/2020-07-13_detect-base64-encoded-messages/change.md)

- 0.1.0-preview

  - Initial record

## Implementations

| Language   | Confirmed Compatible with Spec Version | Minimum Version Confirmed | Implementation                                                                                                                                                 |
| ---------- | -------------------------------------- | ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| C          | 0.2.0                                  | 0.1.0                     | [session_decrypt.c](https://github.com/aws/aws-encryption-sdk-c/blob/master/source/session_decrypt.c)                                                          |
| NodeJS     | 0.2.0                                  | 0.1.0                     | [decrypt.ts](https://github.com/awslabs/aws-encryption-sdk-javascript/blob/master/modules/encrypt-node/src/decrypt.ts)                                         |
| Browser JS | 0.2.0                                  | 0.1.0                     | [decrypt.ts](https://github.com/awslabs/aws-encryption-sdk-javascript/blob/master/modules/encrypt-browser/src/decrypt.ts)                                      |
| Python     | 0.2.0                                  | 1.2.0                     | [streaming_client.py](https://github.com/aws/aws-encryption-sdk-python/blob/master/src/aws_encryption_sdk/streaming_client.py)                                 |
| Java       | 0.2.0                                  | 0.0.1                     | [DecryptionHandler.java](https://github.com/aws/aws-encryption-sdk-java/blob/master/src/main/java/com/amazonaws/encryptionsdk/internal/DecryptionHandler.java) |

## Overview

This document describes the AWS Encryption SDK's (ESDK's) decrypt operation,
used for decrypting a message that was previously encrypted by the ESDK.

Any client provided by the AWS Encryption SDK that performs decryption of encrypted messages MUST follow
this specification for decryption.

## Definitions

### Conventions used in this document

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL"
in this document are to be interpreted as described in [RFC2119](https://tools.ietf.org/html/rfc2119).

### Authenticated Data

Plaintext or associated data is considered authenticated if the associated
[authentication tag](../data-format/message-body.md#authentication-tag) is successfully checked
as defined by the algorithm suite indicated in the message header.

This operation MUST NOT release any unauthenticated plaintext or unauthenticated associated data.

### Signed Data

Plaintext and associated data is considered signed if the associated [message signature](../data-format/message-footer.md)
is successfully verified using the [signature algorithm](../framework/algorithm-suites.md#signature-algorithm)
of the algorithm suite indicated in the message header.

## Input

The client MUST require the following as inputs to this operation:

- [Encrypted Message](#encrypted-message)

The client MUST require exactly one of the following types of inputs:

- [Cryptographic Materials Manager (CMM)](../framework/cmm-interface.md)
- [Keyring](../framework/keyring-interface.md)

### Encrypted Message

The encrypted message to decrypt.
The input encrypted message MUST be a sequence of bytes in the
[message format](../data-format/message.md) specified by the AWS Encryption SDK.
The encrypted message contains the list of [encrypted data keys](../data-format/message-header.md#encrypted-data-keys),
[encryption context](../data-format/message-header.md#aad), if provided during encryption,
[encrypted content](../data-format/message-body.md#encrypted-content) and
[algorithm suite ID](../data-format/message-header.md#algorithm-suite-id) among other metadata.
Each key in the encrypted data key list is an encrypted version of the single plaintext data key that was used to encrypt the plaintext.
The encryption context is the additional authenticated data that was used during encryption.
The algorithm suite ID refers to the algorithm suite used to encrypt the message and is required to decrypt the encrypted message.

This input MAY be [streamed](streaming.md) to this operation.

If an implementation requires holding the entire encrypted message in memory in order to perform this operation,
that implementation SHOULD NOT provide an API that allows the caller to stream the encrypted message.

#### Encrypted Message Format

The message format is a binary format, but it is a common mistake for users to attempt decryption on the Base64 encoding of this data instead.
Because the first two bytes of the message format have a very limited set of possible values
(currently they are in fact fixed),
the first two bytes of the Base64 encoding of a valid message are also simple to recognize.

To make diagnosing this mistake easier, implementations SHOULD detect the first two bytes of the Base64 encoding of any supported message [versions](../data-format/message-header.md#version-1)
and [types](../data-format/message-header.md#type)
and fail with a more specific error message.
In particular, the hex values to detect for the current set of versions and types are:

| Version and type (hex) | Base64 encoding (ascii) | Base64 encoding (hex) |
| ---------------------- | ----------------------- | --------------------- |
| 01 80                  | A Y ...                 | 41 59 ...             |

Note that the bytes that follow the initial two in the Base64 encoding
partially depend on subsequent bytes in the binary message format
and hence are not as predictable.

### Cryptographic Materials Manager

A CMM that implements the [CMM interface](../framework/cmm-interface.md).

This CMM MUST obtain the [decryption materials](../framework/structures.md#decryption-materials) required for decryption.

### Keyring

A Keyring that implements the [keyring interface](../framework/keyring-interface.md).

If the Keyring is provided as the input, the client MUST construct a [default CMM](../framework/default-cmm.md) that uses this keyring,
to obtain the [decryption materials](../framework/structures.md#decryption-materials) that is required for decryption.

This default CMM MUST obtain the decryption materials required for decryption.

## Output

The client MUST return as output to this operation:

- [Plaintext](#plaintext)
- [Encryption Context](#encryption-context)
- [Algorithm Suite](#algorithm-suite)

The client SHOULD return as an output:

- [Parsed Header](#parsed-header)

### Plaintext

The decrypted data.

This operation MAY [stream](streaming.md) the plaintext as output.

If an implementation requires holding the entire encrypted message in memory in order to perform this operation,
that implementation SHOULD NOT provide an API that allows the caller to stream the encrypted message.

### Encryption Context

The [encryption context](../framework/structures.md#encryption-context) that is used as
additional authenticated data during the decryption of the input [encrypted message](#encrypted-message).

This output MAY be satisfied by outputting a [parsed header](#parsed-header) containing this value.

### Algorithm Suite

The [algorithm suite](../framework/algorithm-suites.md) that is used to decrypt
the input [encrypted message](#encrypted-message).

This output MAY be satisfied by outputting a [parsed header](#parsed-header) containing this value.

### Parsed Header

A collection of deserialized fields of the [encrypted message's](#encrypted-message) header.

## Behavior

The Decrypt operation is divided into several distinct steps:

- [Parse the header](#parse-the-header)
- [Get the decryption materials](#get-the-decryption-materials)
- [Verify the header](#verify-the-header)
- [Decrypt the message body](#decrypt-the-message-body)
- [Verify the signature](#verify-the-signature)
  - If the message header contains an algorithm suite including a
    [signature algorithm](../framework/algorithm-suites.md#signature-algorithm),
    this operation MUST perform this step.
    Otherwise this operation MUST NOT perform this step.

This operation MUST perform all the above steps unless otherwise specified,
and it MUST perform them in the above order.

If the input encrypted message is not being [streamed](streaming.md) to this operation,
all output MUST NOT be released until after these steps complete successfully.

If the input encrypted message is being [streamed](streaming.md) to this operation:

- Output MUST NOT be released until otherwise indicated.
- If all bytes have been provided and this operation
  is unable to complete the above steps with the consumable encrypted message bytes,
  this operation MUST halt and indicate a failure to the caller.
- If this operation successfully completes the above steps
  but there are consumable bytes which are intended to be decrypted,
  this operation MUST fail.
- The ESDK MUST provide a configuration option that causes the decryption operation
  to fail immediately after parsing the header if a signed algorithm suite is used.
  This can be used to ensure that data not yet [verified as signed data](#security-considerations)
  is never released early.

### Parse the header

Given encrypted message bytes, this operation MUST process those bytes sequentially,
deserializing those bytes according to the [message format](../data-format/message.md).

This operation MUST attempt to deserialize all consumable encrypted message bytes until it has
successfully deserialized a valid [message header](../data-format/message-header.md).

If the number of [encrypted data keys](../framework/structures.md#encrypted-data-keys)
deserialized from the [message header](../data-format/message-header.md)
is greater than the [maximum number of encrypted data keys](client.md#maximum-number-of-encrypted-data-keys) configured in the [client](client.md),
then as soon as that can be determined during deserializing
decrypt MUST process no more bytes and yield an error.

This operation MUST wait if it doesn't have enough consumable encrypted message bytes to
deserialize the next field of the message header until enough input bytes become consumable or
the caller indicates an end to the encrypted message.

Until the [header is verified](#verify-the-header), this operation MUST NOT
release any parsed information from the header.

### Get the decryption materials

If the parsed [algorithm suite ID](../data-format/message-header.md#algorithm-suite-id)
is not supported by the [commitment policy](client.md#commitment-policy)
configured in the [client](client.md) decrypt MUST yield an error.

To verify the message header and decrypt the message body,
a set of valid decryption materials is required.

This operation MUST obtain this set of [decryption materials](../framework/structures.md#decryption-materials),
by calling [Decrypt Materials](../framework/cmm-interface.md#decrypt-materials) on a [CMM](../framework/cmm-interface.md).

The CMM used MUST be the input CMM, if supplied.
If a CMM is not supplied as the input, the decrypt operation MUST construct a [default CMM](../framework/default-cmm.md)
from the input [keyring](../framework/keyring-interface.md).

The call to the CMM's [Decrypt Materials](../framework/cmm-interface.md#decrypt-materials) operation
MUST be constructed as follows:

- Encryption Context: This is the parsed [encryption context](../data-format/message-header.md#aad)
  from the message header.
- Algorithm Suite ID: This is the parsed
  [algorithm suite ID](../data-format/message-header.md#algorithm-suite-id)
  from the message header.
- Encrypted Data Keys: This is the parsed [encrypted data keys](../data-format/message-header#encrypted-data-keys)
  from the message header.

The data key used as input for all decryption described below is a data key derived from the plaintext data key
included in the [decryption materials](../framework/structures.md#decryption-materials).
The algorithm suite used as input for all decryption described below is a algorithm suite
included in the [decryption materials](../framework/structures.md#decryption-materials).
If the algorithm suite is not supported by the [commitment policy](client.md#commitment-policy)
configured in the [client](client.md) decrypt MUST yield an error.
If the [algorithm suite](../framework/algorithm-suites.md#algorithm-suites-encryption-key-derivation-settings) supports [key commitment](../framework/algorithm-suites.md#key-commitment)
then the [commit key](../framework/algorithm-suites.md#commit-key) MUST be derived from the plaintext data key
using the [commit key derivation](../framework/algorithm-suites.md#algorithm-suites-commit-key-derivation-settings).
The derived commit key MUST equal the commit key stored in the message header.
The algorithm suite used to derive a data key from the plaintext data key MUST be
the [key derivation algorithm](../framework/algorithm-suites.md#key-derivation-algorithm) included in the
[algorithm suite](../framework/algorithm-suites.md) associated with
the returned decryption materials.
This document refers to the output of the key derivation algorithm as the derived data key.
Note that if the key derivation algorithm is the [identity KDF](../framework/algorithm-suites.md#identity-kdf),
then the derived data key is the same as the plaintext data key.

### Verify the header

Once a valid message header is deserialized and decryption materials are available,
this operation MUST validate the [message header body](../data-format/message-header.md#header-body)
by using the [authenticated encryption algorithm](../framework/algorithm-suites.md#encryption-algorithm)
to decrypt with the following inputs:

- the AAD is the serialized [message header body](../data-format/message-header.md#header-body).
- the IV is the value serialized in the message header's [IV field](../data-format/message-header#iv).
- the cipherkey is the derived data key
- the ciphertext is an empty byte array
- the tag is the value serialized in the message header's
  [authentication tag field](../data-format/message-header.md#authentication-tag)

If this tag verification fails, this operation MUST immediately halt and fail.

If the input encrypted message is being [streamed](streaming.md) to this operation:

- This operation SHOULD release the parsed [encryption context](#encryption-context),
  [algorithm suite ID](../data-format/message-header.md#algorithm-suite-id),
  and [other header information](#parsed-header)
  as soon as tag verification succeeds.
  However, if this operation is using an algorithm suite with a signature algorithm
  all released output MUST NOT be considered signed data until
  this operation successfully completes.
  See [security considerations](#security-considerations) below.
- This operation SHOULD input the serialized header to the signature algorithm as soon as it is deserialized,
  such that the serialized frame isn't required to remain in memory to [verify the signature](#verify-the-signature).

### Decrypt the message body

Once the message header is successfully parsed, the next sequential bytes
MUST be deserialized according to the [message body spec](../data-format/message-body.md).

While there MAY still be message body left to deserialize and decrypt,
this operation MUST either wait for more of the encrypted message bytes to become consumable,
wait for the end to the encrypted message to be indicated,
or to deserialize and/or decrypt the consumable bytes.

The [content type](../data-format/message-header.md#content-type) field parsed from the
message header above determines whether these bytes MUST be deserialized as
[framed data](../data-format/message-body.md#framed-data) or
[un-framed data](../data-format/message-body.md#un-framed-data).

If deserializing [framed data](../data-format/message-body.md#framed-data),
this operation MUST use the first 4 bytes of a frame to determine if the frame
MUST be deserialized as a [final frame](../data-format/message-body.md#final-frame)
or [regular frame](../fata-format/message-body/md#regular-frame).
If the first 4 bytes have a value of 0xFFFF,
then this MUST be deserialized as the [sequence number end](../data-format/message-header.md#sequence-number-end)
and the following bytes according to the [final frame spec](../data-format/message-body.md#final-frame).
Otherwise, this MUST be deserialized as the [sequence number](../data-format/message-header.md#sequence-number)
and the following bytes according to the [regular frame spec](../data-format/message-body.md#regular-frame).

If deserializing a [final frame](../data-format/message-body.md#final-frame),
this operation MUST ensure that the length of the encrypted content field is
less than or equal to the frame length deserialized in the message header.

Once at least a single frame is deserialized (or the entire body in the un-framed case),
this operation MUST decrypt and authenticate the frame (or body) using the
[authenticated encryption algorithm](../framework/algorithm-suites.md#encryption-algorithm)
specified by the [algorithm suite](../framework/algorithm-suites.md), with the following inputs:

- The AAD is the serialized [message body AAD](../data-format/message-body-aad.md),
  constructed as follows:
  - The [message ID](../data-format/message-body-aad.md#message-id) is the same as the
    [message ID](../data-frame/message-header.md#message-id) deserialized from the header of this message.
  - The [Body AAD Content](../data-format/message-body-aad.md#body-aad-content) depends on
    whether the thing being decrypted is a regular frame, final frame, or un-framed data.
    Refer to [Message Body AAD](../data-format/message-body-aad.md) specification for more information.
  - The [sequence number](../data-format/message-body-aad.md#sequence-number) is the sequence
    number deserialized from the frame being decrypted.
    If this is un-framed data, this value MUST be 1.
    If this is framed data and the first frame sequentially, this value MUST be 1.
    Otherwise, this value MUST be 1 greater than the value of the sequence number
    of the previous frame.
  - The [content length](../data-format/message-body-aad.md#content-length) MUST have a value
    equal to the length of the plaintext that was encrypted.
    This can be determined by using the [frame length](../data-format/message-header.md#frame-length)
    deserialized from the message header if this is a regular frame,
    or the [encrypted content length](../data-format/message-body.md#encrypted-content-length)
    otherwise.
- The IV is the [sequence number](../data-format/message-body-aad.md#sequence-number)
  used in the message body AAD above,
  padded to the [IV length](../data-format/message-header.md#iv-length) with 0.
- The cipherkey is the derived data key
- The ciphertext is the [encrypted content](../data-format/message-body.md#encrypted-content).
- the tag is the value serialized in the
  [authentication tag field](../data-format/message-body.md#authentication-tag)
  in the message body or frame.

If this decryption fails, this operation MUST immediately halt and fail.
This operation MUST NOT release any unauthenticated plaintext.

If the input encrypted message is being [streamed](streaming.md) to this operation:

- If this operation is using an algorithm suite without a signature algorithm,
  plaintext SHOULD be released as soon as the above calculation, including tag verification,
  succeeds.
- If this operation is using an algorithm suite with a signature algorithm,
  all plaintext decrypted from regular frames SHOULD be released as soon as the above calculation,
  including tag verification, succeeds.
  Any plaintext decrypted from [unframed data](../data-format/message-body.md#un-framed-data) or
  a final frame MUST NOT be released until [signature verification](#verify-the-signature)
  successfully completes.
- This operation SHOULD input the serialized frame to the signature algorithm as soon as it is deserialized,
  such that the serialized frame isn't required to remain in memory to complete
  the [signature verification](#verify-the-signature).

### Verify the signature

If the algorithm suite has a signature algorithm,
this operation MUST verify the message footer using the specified signature algorithm.

After deserializing the body, this operation MUST deserialize the next encrypted message bytes
as the [message footer](../data-format/message-footer.md).

If there are not enough consumable bytes to deserialize the message footer and
the caller has not yet indicated an end to the encrypted message,
this operation MUST wait for enough bytes to become consumable or for the caller
to indicate an end to the encrypted message.

Once the message footer is deserialized, this operation MUST use the
[signature algorithm](../framework/algorithm-suites.md#signature-algorithm)
from the [algorithm suite](../framework/algorithm-suites.md) in the decryption materials to
verify the encrypted message, with the following inputs:

- The verification key is the [verification key](../framework/structures.md#verification-key)
  in the decryption materials.
- The input to verify is the concatenation of the serialization of the
  [message header](../data-format/message-header.md) and [message body](../data-format/message-body.md).

Note that the message header and message body MAY have already been input during previous steps.

If this verification is not successful, this operation MUST immediately halt and fail.

## Security Considerations

If this operation is [streaming](streaming.md) output to the caller
and is decrypting messages created with an algorithm suite including a signature algorithm,
any released plaintext MUST NOT be considered signed data until this operation finishes
successfully.

This means that callers that process such released plaintext MUST NOT consider any processing successful
until this operation completes successfully.
Additionally, if this operation fails, callers MUST discard the released plaintext and encryption context
and MUST rollback any processing done due to the released plaintext or encryption context.
