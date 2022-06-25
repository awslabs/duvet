# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Annotation Parser used by duvet-python."""
# import logging
# from pathlib import Path
from test.utils import populate_file

import pytest

from duvet.annotation_parser import AnnotationParser, LineSpan

# from typing import Iterable


pytestmark = [pytest.mark.unit, pytest.mark.local]

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


def test_extract_blocks(tmp_path):
    actual_path = populate_file(tmp_path, TEST_DFY_BLOCK, "src/test-duvet/test-duvet.dfy")
    lines = TEST_DFY_BLOCK.splitlines(keepends=True)
    # lines[-1] = lines[-1] = +   '\n'
    parser = AnnotationParser([actual_path])
    actual_linespan = parser._extract_blocks(lines)
    assert actual_linespan == [LineSpan(start=0, end=6)]


def test_extract_blocks_nested(tmp_path):
    actual_path = populate_file(tmp_path, ANNOTATION_NESTED_IN_FUNCTION, "src/test-duvet/test-duvet.dfy")
    lines = ANNOTATION_NESTED_IN_FUNCTION.splitlines(keepends=True)
    parser = AnnotationParser([actual_path])
    actual_linespan = parser._extract_blocks(lines)
    assert actual_linespan == [LineSpan(start=3, end=12)]


def test_extract_anno_kwargs(tmp_path):
    actual_path = populate_file(tmp_path, TEST_DFY_BLOCK, "src/test-duvet/test-duvet.dfy")
    lines = TEST_DFY_BLOCK.splitlines(keepends=True)
    parser = AnnotationParser([actual_path])
    line_span = LineSpan(0, 6)
    actual_kwargs = parser._extract_anno_kwargs(lines, [line_span])
    assert actual_kwargs == [
        {
            "content": "On client initialization, the caller MUST have the option to "
                       "provide\n"
                       "a:\n"
                       "*  commitment policy (Section 2.4.1)\n"
                       "*  maximum number of encrypted data keys (Section 2.4.2)\n",
            "end": 6,
            "reason": None,
            "start": 0,
            "target": "compliance/client-apis/client.txt#2.4",
            "type": "implication",
        }
    ]


def test_process_file(tmp_path):
    actual_path = populate_file(tmp_path, TEST_DFY_BLOCK, "src/test-duvet/test-duvet.dfy")
    parser = AnnotationParser([actual_path])
    actual_dicts = parser.process_file(actual_path)
    assert actual_dicts == [
        {
            "content": "On client initialization, the caller MUST have the option to "
                       "provide\n"
                       "a:\n"
                       "*  commitment policy (Section 2.4.1)\n"
                       "*  maximum number of encrypted data keys (Section 2.4.2)\n",
            "end": 6,
            "reason": None,
            "start": 0,
            "target": "compliance/client-apis/client.txt#2.4",
            "type": "implication",
        }
    ]


def test_process_file_with_nested_annotation(tmp_path):
    actual_path = populate_file(tmp_path, ANNOTATION_NESTED_IN_FUNCTION, "src/test-duvet/test-duvet.dfy")
    parser = AnnotationParser([actual_path])
    actual_dicts = parser.process_file(actual_path)
    assert actual_dicts == [
        {
            "content": "The IV length MUST be equal to the IV\n"
                       "length of the algorithm suite specified by the Algorithm Suite "
                       "ID\n"
                       "(message-header.md#algorithm-suite-id) field.\n",
            "end": 8,
            "reason": None,
            "start": 3,
            "target": "compliance/data-format/message-body.txt#2.5.2.1.2",
            "type": "implication",
        },
        {
            "content": "The IV length MUST be equal to the IV length of the algorithm "
                       "suite\n"
                       "(../framework/algorithm-suites.md) that generated the message.\n",
            "end": 12,
            "reason": None,
            "start": 8,
            "target": "compliance/data-format/message-body.txt#2.5.2.2.3",
            "type": "implication",
        },
    ]


def test_process_anno_kwargs(tmp_path):
    actual_path = populate_file(tmp_path, ANNOTATION_NESTED_IN_FUNCTION, "src/test-duvet/test-duvet.dfy")
    parser = AnnotationParser([actual_path])
    actual_dicts = [
        {
            "content": "The IV length MUST be equal to the IV\n"
                       "length of the algorithm suite specified by the Algorithm Suite "
                       "ID\n"
                       "(message-header.md#algorithm-suite-id) field.\n",
            "end": 8,
            "reason": None,
            "start": 3,
            "target": "compliance/data-format/message-body.txt#2.5.2.1.2",
            "type": "implication",
        },
        {
            "content": "The IV length MUST be equal to the IV length of the algorithm "
                       "suite\n"
                       "(../framework/algorithm-suites.md) that generated the message.\n",
            "end": 12,
            "reason": None,
            "start": 8,
            "target": "compliance/data-format/message-body.txt#2.5.2.2.3",
            "type": "implication",
        },
    ]
    actual_annos = parser._process_anno_kwargs(actual_dicts, actual_path)
    assert len(actual_annos) == 2
    assert actual_annos[0].anno_type.name == "IMPLICATION"
    assert actual_annos[1].anno_type.name == "IMPLICATION"
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


def test_validate_anno_kwarg(tmp_path):
    actual_path = populate_file(tmp_path, ANNOTATION_NESTED_IN_FUNCTION, "src/test-duvet/test-duvet.dfy")
    parser = AnnotationParser([actual_path])
    actual_dict = {
        "content": "The IV length MUST be equal to the IV length of the algorithm "
                   "suite\n"
                   "(../framework/algorithm-suites.md) that generated the message.\n",
        "end": 12,
        "reason": None,
        "start": 8,
        "target": "compliance/data-format/message-body.txt#2.5.2.2.3",
        "type": "implication",
    }
    actual_anno = parser._validate_anno_kwarg(actual_dict, tmp_path)
    assert actual_anno.target == "compliance/data-format/message-body.txt#2.5.2.2.3"
    assert actual_anno.anno_type.name == "IMPLICATION"
    assert (
            actual_anno.content == "The IV length MUST be equal to the IV length of the algorithm "
                                   "suite (../framework/algorithm-suites.md) that generated the message."
    )

    #
    # def test_esdk_compliance_exceptions():
    #     logging.basicConfig(level=logging.INFO)
    #
    #     esdk_dafny = Path("/Users/tonyknap/workplace/Polymorph/aws-encryption-sdk-dafny")
    #     excep_paths: Iterable[Path] = esdk_dafny.glob("compliance_exceptions/**/*.txt")
    #     list_list_kwargs: list[list[dict]] = []
    #     a_parser = AnnotationParser(paths=excep_paths)
    #     for filepath in a_parser.paths:
    #         try:
    #             list_list_kwargs.append(a_parser.process_file(filepath))
    #         except KeyboardInterrupt:
    #             break
    #         except Exception as ex:
    #             _LOGGER.error("%s: hit %s.", (str(filepath), ex), ex)
    #
    #
    # def test_end_file():
    #     annotation_end_of_file = """
    #       //= compliance/data-format/message-body.txt#2.5.2.1.2
    #       //= type=implication
    #       //# Each frame in the Framed Data (Section 2.5.2) MUST include an IV that
    #       //# is unique within the message.
    #       //
    #       //= compliance/data-format/message-body.txt#2.5.2.2.3
    #       //= type=implication
    #       //# The IV MUST be a unique IV within the message."""
    #     filepath: Path = Path(
    #         "/Users/tonyknap/workplace/Polymorph/aws-encryption-sdk-dafny/src"
    #         "/AwsCryptographicMaterialProviders/Client.dfy"
    #     )
    #     a_parser = AnnotationParser(paths=[filepath])
    #     lines: list[str] = annotation_end_of_file.split("\n")
    #     lines = [line + "\n" for line in lines]
    #     anno_blocks: list[LineSpan] = a_parser._extract_blocks(lines)
    #     kwargs: list[dict] = a_parser._extract_anno_kwargs(lines, anno_blocks)
    #
    #
    # def test_double_block():
    #     annotation_nested_in_function = """
    #       function method IVSeq(suite: Client.AlgorithmSuites.AlgorithmSuite, sequenceNumber: uint32)
    #         :(ret: seq<uint8>)
    #         //= compliance/data-format/message-body.txt#2.5.2.1.2
    #         //= type=implication
    #         //# The IV length MUST be equal to the IV
    #         //# length of the algorithm suite specified by the Algorithm Suite ID
    #         //# (message-header.md#algorithm-suite-id) field.
    #         //= compliance/data-format/message-body.txt#2.5.2.2.3
    #         //= type=implication
    #         //# The IV length MUST be equal to the IV length of the algorithm suite
    #         //# (../framework/algorithm-suites.md) that generated the message.
    #         ensures |ret| == suite.encrypt.ivLength as nat
    #       {
    #         seq(suite.encrypt.ivLength as int - 4, _ => 0) + UInt32ToSeq(sequenceNumber)
    #       }"""
    #     filepath: Path = Path(
    #         "/Users/tonyknap/workplace/Polymorph/aws-encryption-sdk-dafny/src"
    #         "/AwsCryptographicMaterialProviders/Client.dfy"
    #     )
    #     a_parser = AnnotationParser(paths=[filepath])
    #     lines: list[str] = annotation_nested_in_function.split("\n")
    #     lines = [line + "\n" for line in lines]
    #     anno_blocks: list[LineSpan] = a_parser._extract_blocks(lines)
    #     kwargs: list[dict] = a_parser._extract_anno_kwargs(lines, anno_blocks)
