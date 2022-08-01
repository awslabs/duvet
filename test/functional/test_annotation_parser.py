# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Functional testing for annotation parsing"""
import pytest

from duvet.annotation_parser import AnnotationParser

from ..utils import populate_file  # isort: skip

pytestmark = [pytest.mark.local, pytest.mark.functional]

TEST_DFY_BLOCK = """        //= compliance/client-apis/client.txt#2.4
        //= type=implication
        //# On client initialization, the caller MUST have the option to provide
        //# a:
        //#*  commitment policy (Section 2.4.1)
        //#*  maximum number of encrypted data keys (Section 2.4.2)
"""

ANNOTATION_NESTED_IN_FUNCTION = """
  function method IVSeq(suite: Client.AlgorithmSuites.AlgorithmSuite, sequenceNumber: uint32)
    :(ret: seq<uint8>)
    //= compliance/data-format/message-body.txt#2.5.2.1.2
    //= type=implication
    //# The IV length MUST be equal to the IV
    //# length of the algorithm suite specified by the Algorithm Suite ID
    //# (message-header.md#algorithm-suite-id) field.
    //= compliance/data-format/message-body.txt#2.5.2.2.3
    //= type=implication
    //# The IV length MUST be equal to the IV length of the algorithm suite
    //# (../framework/algorithm-suites.md) that generated the message.
    ensures |ret| == suite.encrypt.ivLength as nat
  {
    seq(suite.encrypt.ivLength as int - 4, _ => 0) + UInt32ToSeq(sequenceNumber)
  }"""

ANNOTATION_END_OF_FILE = """
  //= compliance/data-format/message-body.txt#2.5.2.1.2
  //= type=implication
  //# Each frame in the Framed Data (Section 2.5.2) MUST include an IV that
  //# is unique within the message.
  //
  //= compliance/data-format/message-body.txt#2.5.2.2.3
  //= type=implication
  //# The IV MUST be a unique IV within the message."""

IMPLICATION = "IMPLICATION"


# //= compliance/duvet-specification.txt#2.5.2
# //= type=test
# //# A specification requirement MUST be labeled "Attested" if there exists at least one matching annotation of type

# //= compliance/duvet-specification.txt#2.5.4
# //= type=test
# //# A specification requirement MUST be labeled "Unexcused" and MUST only be labeled "Unexcused"
# //# if there exists a matching annotation of type "exception" and the annotation does NOT have a "reason".


def test_more_than_one_valid_files(tmp_path):
    actual_path1 = populate_file(tmp_path, TEST_DFY_BLOCK, "src/test-duvet/test-duvet1.dfy")
    actual_path2 = populate_file(tmp_path, ANNOTATION_NESTED_IN_FUNCTION, "src/test-duvet/test-duvet2.dfy")
    parser = AnnotationParser([actual_path1, actual_path2])
    actual_annotations = parser.process_all()
    assert len(actual_annotations) == 3


def test_extract_python_no_implementation_annotation(pytestconfig):
    path = pytestconfig.rootpath.joinpath("src/duvet/__init__.py")
    anno_meta_style = "# //="
    anno_content_style = "# //#"
    actual_parser = AnnotationParser([path], anno_meta_style, anno_content_style)
    assert actual_parser.meta_style == anno_meta_style
    assert actual_parser.content_style == anno_content_style
    assert len(actual_parser.process_all()) == 0


def test_annotation_end_a_file(tmp_path):
    actual_path = populate_file(tmp_path, ANNOTATION_END_OF_FILE, "src/test-duvet/test-annotation-ends-a-file.dfy")
    parser = AnnotationParser([actual_path])
    actual_annotations = parser.process_file(actual_path)
    assert len(actual_annotations) == 2
    # Verify the last annotation is not broken.
    assert actual_annotations[1].type.name == "IMPLICATION"
    assert actual_annotations[1].target == "compliance/data-format/message-body.txt#2.5.2.2.3"
    assert actual_annotations[1].content == ("The IV MUST be a unique IV within the message.")
    assert (
        actual_annotations[1].uri == "compliance/data-format/message-body.txt#2.5.2.2.3$The IV MUST be a unique IV "
        "within the message."
    )
