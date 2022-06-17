# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Functional testing for annotation parsing"""
import pathlib

import pytest

from duvet.annotation_parser import AnnotationParser

from ..utils import populate_file  # isort:skip

pytestmark = [pytest.mark.local, pytest.mark.functional]

TEST_DFY_BLOCK = """// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

include "../StandardLibrary/StandardLibrary.dfy"
include "../StandardLibrary/UInt.dfy"
include "Serialize/SerializableTypes.dfy"
include "../AwsCryptographicMaterialProviders/Client.dfy"
include "../Crypto/AESEncryption.dfy"
include "../Util/Streams.dfy"
include "../Util/UTF8.dfy"
include "Serialize/Frames.dfy"

include "Serialize/Header.dfy"
include "Serialize/HeaderTypes.dfy"
include "Serialize/V1HeaderBody.dfy"
include "Serialize/HeaderAuth.dfy"
include "Serialize/SerializeFunctions.dfy"
include "../../libraries/src/Collections/Sequences/Seq.dfy"

module MessageBody {
  // export
  //   provides EncryptMessageBody, DecryptFramedMessageBody, DecryptNonFramedMessageBody,
  //     Wrappers, UInt, Streams, Client,
  //     FramesEncryptPlaintext, AESEncryption, DecryptedWithKey
  //   reveals  SeqWithGhostFrames

  import opened Wrappers
  import opened UInt = StandardLibrary.UInt
  import AESEncryption
  import Streams
  import UTF8
  import SerializableTypes
  import MaterialProviders.Client
  import Frames
  import Header
  import opened SerializeFunctions
  import Seq
  import StandardLibrary

  datatype BodyAADContent = AADRegularFrame | AADFinalFrame | AADSingleBlock

  const BODY_AAD_CONTENT_REGULAR_FRAME: string := "AWSKMSEncryptionClient Frame"
  const BODY_AAD_CONTENT_FINAL_FRAME: string := "AWSKMSEncryptionClient Final Frame"
  const BODY_AAD_CONTENT_SINGLE_BLOCK: string := "AWSKMSEncryptionClient Single Block"

  function method BodyAADContentTypeString(bc: BodyAADContent): string {
    match bc
    case AADRegularFrame => BODY_AAD_CONTENT_REGULAR_FRAME
    case AADFinalFrame => BODY_AAD_CONTENT_FINAL_FRAME
    case AADSingleBlock => BODY_AAD_CONTENT_SINGLE_BLOCK
  }

  const START_SEQUENCE_NUMBER: uint32 := Frames.START_SEQUENCE_NUMBER
  const ENDFRAME_SEQUENCE_NUMBER: uint32 := Frames.ENDFRAME_SEQUENCE_NUMBER
  const NONFRAMED_SEQUENCE_NUMBER: uint32 := Frames.NONFRAMED_SEQUENCE_NUMBER

  function method IVSeq(suite: Client.AlgorithmSuites.AlgorithmSuite, sequenceNumber: uint32)
    :(ret: seq<uint8>)
    //= compliance/data-format/message-body.txt#2.5.2.1.2
    //= type=implication
    //# The IV length MUST be equal to the IV
    //# length of the algorithm suite specified by the Algorithm Suite ID
    //# (message-header.md#algorithm-suite-id) field.
    //
    //= compliance/data-format/message-body.txt#2.5.2.2.3
    //= type=implication
    //# The IV length MUST be equal to the IV length of the algorithm suite
    //# (../framework/algorithm-suites.md) that generated the message.
    ensures |ret| == suite.encrypt.ivLength as nat
  {
    seq(suite.encrypt.ivLength as int - 4, _ => 0) + UInt32ToSeq(sequenceNumber)
  }

  //= compliance/data-format/message-body.txt#2.5.2.1.2
  //= type=implication
  //# Each frame in the Framed Data (Section 2.5.2) MUST include an IV that
  //# is unique within the message.
  //
  //= compliance/data-format/message-body.txt#2.5.2.2.3
  //= type=implication
  //# The IV MUST be a unique IV within the message.
  lemma IVSeqDistinct(suite: Client.AlgorithmSuites.AlgorithmSuite, m: uint32, n: uint32)
    requires m != n
    ensures
      var algorithmSuiteID := SerializableTypes.GetESDKAlgorithmSuiteId(suite.id);
      && IVSeq(suite, m) != IVSeq(suite, n)
  {
    var paddingLength :=  suite.encrypt.ivLength as int - 4;
    assert IVSeq(suite, m)[paddingLength..] == UInt32ToSeq(m);
    assert IVSeq(suite, n)[paddingLength..] == UInt32ToSeq(n);
    UInt32SeqSerializeDeserialize(m);
    UInt32SeqSerializeDeserialize(n);
  }
"""


def test_one_valid_file(tmp_path):
    actual_path = populate_file(tmp_path, TEST_DFY_BLOCK, "src/test-duvet/test-duvet.dfy")
    actual_annos = AnnotationParser([actual_path])._extract_file_annotations(actual_path)
    assert len(actual_annos) == 4
    assert actual_annos[0].type.name == "IMPLICATION"
    assert actual_annos[0].target == "compliance/data-format/message-body.txt#2.5.2.1.2"
    assert actual_annos[0].content == (
        "The IV length MUST be equal to the IVlength of the algorithm suite specified "
        "by the Algorithm Suite ID(message-header.md#algorithm-suite-id) field."
    )
    assert actual_annos[0].uri == (
        "compliance/data-format/message-body.txt#2.5.2.1.2$The IV length MUST be "
        "equal to the IVlength of the algorithm suite specified by the Algorithm "
        "Suite ID(message-header.md#algorithm-suite-id) field."
    )


def test_2_valid_file(tmp_path):
    actual_path1 = populate_file(tmp_path, TEST_DFY_BLOCK, "src/test-duvet/test-duvet1.dfy")
    actual_path2 = populate_file(tmp_path, TEST_DFY_BLOCK, "src/test-duvet/test-duvet2.dfy")
    actual_annos = AnnotationParser([actual_path1, actual_path2]).extract_implementation_file_annotations()
    assert len(actual_annos) == 8
    assert actual_annos[0].type.name == "IMPLICATION"  # pylint: disable=(unsubscriptable-object
    assert (
        actual_annos[0].target  # pylint: disable=(unsubscriptable-object
        == "compliance/data-format/message-body.txt#2.5.2.1.2"
    )
    assert actual_annos[0].content == (  # pylint: disable=(unsubscriptable-object
        "The IV length MUST be equal to the IVlength of the algorithm suite specified "
        "by the Algorithm Suite ID(message-header.md#algorithm-suite-id) field."
    )
    assert actual_annos[0].uri == (  # pylint: disable=(unsubscriptable-object
        "compliance/data-format/message-body.txt#2.5.2.1.2$The IV length MUST be "
        "equal to the IVlength of the algorithm suite specified by the Algorithm "
        "Suite ID(message-header.md#algorithm-suite-id) field."
    )


def test_extract_python_implementation_annotation():
    path = pathlib.Path("./src/duvet/annotation_parser.py").resolve()
    anno_meta_style = "# //="
    anno_content_style = "# //#"
    actual_annos = AnnotationParser(
        [path], anno_meta_style, anno_content_style
    ).extract_implementation_file_annotations()
    # Verify two annotation is added to parser
    assert len(actual_annos) == 2
    assert actual_annos[0].type.name == "IMPLICATION"  # pylint: disable=(unsubscriptable-object
    assert (
        actual_annos[0].target == "compliance/duvet-specification.txt#2.3.1"  # pylint: disable=(unsubscriptable-object
    )
    assert (
        actual_annos[0].content  # pylint: disable=(unsubscriptable-object
        == 'If a second meta line exists it MUST start with "type=".'
    )
    assert actual_annos[0].uri == (  # pylint: disable=(unsubscriptable-object
        "compliance/duvet-specification.txt#2.3.1$If a second meta line exists it " 'MUST start with "type=".'
    )


def test_extract_python_no_implementation_annotation():
    path = pathlib.Path("./src/duvet/identifiers.py").resolve()
    anno_meta_style = "# //="
    anno_content_style = "# //#"
    # Verify warning
    with pytest.warns(UserWarning) as record:
        AnnotationParser([path], anno_meta_style, anno_content_style).extract_implementation_file_annotations()
    # check that only one warning was raised
    assert len(record) == 1
    # check that the message matches
    assert record[0].message.args[0] == (
        "/Users/yuancc/Documents/GitHub/duvet-1/src/duvet/identifiers.py do not have any " "annotations. Skipping file"
    )
