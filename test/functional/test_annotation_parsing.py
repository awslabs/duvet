# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Functional testing for annotation parsing"""

import pytest

# from duvet.annotation_parser import AnnotationParser

# from ..utils import populate_file  # isort:skip
from duvet.annotation_parser import AnnotationParser
from utils import populate_file

pytestmark = [pytest.mark.local, pytest.mark.functional]

# pylint: disable=E1136

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


def test_one_valid_file(tmp_path):
    actual_path = populate_file(tmp_path, TEST_DFY_BLOCK, "src/test-duvet/test-duvet.dfy")
    parser = AnnotationParser([actual_path])
    actual_annos = parser.process_file(actual_path)
    assert len(actual_annos) == 1
    assert actual_annos[0].type.name == "IMPLICATION"
    assert actual_annos[0].target == "compliance/client-apis/client.txt#2.4"
    assert actual_annos[0].content == ('On client initialization, the caller MUST have the option to provide a: *  '
                                       'commitment policy (Section 2.4.1) *  maximum number of encrypted data keys '
                                       '(Section 2.4.2)')
    # assert str(actual_annos[0].location.resolve()) == f"{str(tmp_path.resolve())}/src/test-duvet/test-duvet.dfy"


def test_2_valid_file(tmp_path):
    actual_path1 = populate_file(tmp_path, TEST_DFY_BLOCK, "src/test-duvet/test-duvet1.dfy")
    actual_path2 = populate_file(tmp_path, ANNOTATION_NESTED_IN_FUNCTION, "src/test-duvet/test-duvet2.dfy")
    parser = AnnotationParser([actual_path1, actual_path2])
    actual_annos = parser.process_all()
    assert len(actual_annos) == 3
    assert actual_annos[0].type.name == "IMPLICATION"  # pylint: disable=(unsubscriptable-object
    assert actual_annos[1].type.name == "IMPLICATION"  # pylint: disable=(unsubscriptable-object
    assert actual_annos[2].type.name == "IMPLICATION"  # pylint: disable=(unsubscriptable-object
    assert actual_annos[0].target == "compliance/client-apis/client.txt#2.4"  # pylint: disable=(unsubscriptable-object
    assert actual_annos[0].content == (
        'On client initialization, the caller MUST have the option to provide a: *  '
        'commitment policy (Section 2.4.1) *  maximum number of encrypted data keys '
        '(Section 2.4.2)')
    # assert actual_annos[0].uri == (  # pylint: disable=(unsubscriptable-object
    #     "compliance/client-apis/client.txt#2.4$On client initialization,
    #     "the caller MUST have the option to provide a:"
    # )


def test_extract_python_implementation_annotation(pytestconfig):
    actual_path = pytestconfig.rootpath.joinpath("src/duvet/annotation_parser.py")
    anno_meta_style = "# //="
    anno_content_style = "# //#"
    parser = AnnotationParser([actual_path], anno_meta_style, anno_content_style)
    actual_annos = parser.process_file(actual_path)
    # Verify two annotation is added to parser
    assert len(actual_annos) == 1
    assert actual_annos[0].type.name == "IMPLICATION"  # pylint: disable=(unsubscriptable-object
    assert (
            actual_annos[0].target
            == "compliance/duvet-specification.txt#2.3.1"
        # pylint: disable=(unsubscriptable-object
    )
    assert (
            actual_annos[0].content  # pylint: disable=(unsubscriptable-object
            == 'This identifier of meta parts MUST be configurable.'
    )
    assert actual_annos[0].uri == (  # pylint: disable=(unsubscriptable-object
        'compliance/duvet-specification.txt#2.3.1$This identifier of meta parts MUST be configurable.'
    )


def test_extract_python_no_implementation_annotation(pytestconfig):
    path = pytestconfig.rootpath.joinpath("src/duvet/identifiers.py")
    anno_meta_style = "# //="
    anno_content_style = "# //#"
    AnnotationParser([path], anno_meta_style, anno_content_style)


def test_run_into_another(tmp_path):
    actual_path = populate_file(
        tmp_path, ANNOTATION_NESTED_IN_FUNCTION, "src/test-duvet/test-run-into-another-annotation.dfy"
    )
    parser = AnnotationParser([actual_path])
    actual_annos = parser.process_file(actual_path)
    assert len(actual_annos) == 2
    assert actual_annos[0].type.name == "IMPLICATION"
    assert actual_annos[1].type.name == "IMPLICATION"
    assert actual_annos[0].target == "compliance/data-format/message-body.txt#2.5.2.1.2"
    assert actual_annos[1].target == "compliance/data-format/message-body.txt#2.5.2.2.3"
    assert (
            actual_annos[0].content
            == "The IV length MUST be equal to the IV length of the algorithm suite specified by the "
               "Algorithm Suite ID (message-header.md#algorithm-suite-id) field."
    )
    # Verify the last annotation is not broken.
    assert (
            actual_annos[1].content == "The IV length MUST be equal to the IV length of the algorithm "
                                       "suite (../framework/algorithm-suites.md) that generated the message."
    )
    assert actual_annos[0].uri == (
        "compliance/data-format/message-body.txt#2.5.2.1.2$The IV length MUST be "
        "equal to the IV length of the algorithm suite specified by the Algorithm "
        "Suite ID (message-header.md#algorithm-suite-id) field."
    )


def test_annotation_end_a_file(tmp_path):
    actual_path = populate_file(tmp_path, ANNOTATION_END_OF_FILE, "src/test-duvet/test-sannotation-ends-a-file.dfy")
    parser = AnnotationParser([actual_path])
    actual_annos = parser.process_file(actual_path)
    assert len(actual_annos) == 2
    assert actual_annos[0].type.name == "IMPLICATION"
    assert actual_annos[1].type.name == "IMPLICATION"
    assert actual_annos[0].target == "compliance/data-format/message-body.txt#2.5.2.1.2"
    assert actual_annos[1].target == "compliance/data-format/message-body.txt#2.5.2.2.3"
    assert (
            actual_annos[0].content
            == "Each frame in the Framed Data (Section 2.5.2) MUST include an IV that is unique within "
               "the message."
    )
    # Verify the last annotation is not broken.
    assert actual_annos[1].content == ("The IV MUST be a unique IV within the message.")
    assert (
            actual_annos[0].uri == "compliance/data-format/message-body.txt#2.5.2.1.2$Each frame in the Framed Data "
                                   "(Section 2.5.2) MUST include an IV that is unique within the message."
    )
    assert (
            actual_annos[1].uri == "compliance/data-format/message-body.txt#2.5.2.2.3$The IV MUST be a unique IV "
                                   "within the message."
    )


def test_annotation_only(tmp_path):
    actual_path = populate_file(
        tmp_path, "\n".join([TEST_DFY_BLOCK, ANNOTATION_END_OF_FILE]), "src/test-duvet/test-annotation-only.dfy"
    )
    parser = AnnotationParser([actual_path])
    actual_annos = parser.process_file(actual_path)
    assert len(actual_annos) == 3
    assert actual_annos[0].type.name == "IMPLICATION"
    assert actual_annos[1].type.name == "IMPLICATION"
    assert actual_annos[2].type.name == "IMPLICATION"
    assert actual_annos[0].target == "compliance/client-apis/client.txt#2.4"
    assert actual_annos[1].target == "compliance/data-format/message-body.txt#2.5.2.1.2"
    assert actual_annos[2].target == "compliance/data-format/message-body.txt#2.5.2.2.3"
    assert (
            actual_annos[1].content
            == "Each frame in the Framed Data (Section 2.5.2) MUST include an IV that is unique within "
               "the message."
    )
    assert (
            actual_annos[1].uri == "compliance/data-format/message-body.txt#2.5.2.1.2$Each frame in the Framed Data "
                                   "(Section 2.5.2) MUST include an IV that is unique within the message."
    )
    assert (
            actual_annos[2].uri == "compliance/data-format/message-body.txt#2.5.2.2.3$The IV MUST be a unique IV "
                                   "within the message."
    )
    # Verify the last annotation is not broken.
    assert actual_annos[2].content == ("The IV MUST be a unique IV within the message.")
