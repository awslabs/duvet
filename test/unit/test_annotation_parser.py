# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Annotation Parser used by duvet-python."""
import pathlib

import pytest

from duvet.annotation_parser import AnnotationParser

pytestmark = [pytest.mark.unit, pytest.mark.local]

TEST_DAFNY_STR = (
    "//= compliance/client-apis/client.txt#2.4.2.1\n"
    "//= type=implication\n"
    "//# * encrypt (encrypt.md) MUST only support algorithm suites that have\n"
    "//# a Key Commitment (../framework/algorithm-suites.md#algorithm-\n"
    "//# suites-encryption-key-derivation-settings) value of False\n"
)

expected_content = (
    "* encrypt (encrypt.md) MUST only support algorithm suites that have\n"
    "a Key Commitment (../framework/algorithm-suites.md#algorithm-\n"
    ""
    "suites-encryption-key-derivation-settings) value of False\n"
).replace("\n", "")


def test_extract_annotation():
    """Test a valid annotation string in dafny format."""
    actual_anno = AnnotationParser([pathlib.Path("test.dfy")])._extract_annotation(
        TEST_DAFNY_STR, 0, 5, pathlib.Path("test.dfy")
    )
    assert actual_anno.type.name == "IMPLICATION"
    assert actual_anno.target == "compliance/client-apis/client.txt#2.4.2.1"
    assert actual_anno.content == expected_content
    assert actual_anno.uri == "$".join(["compliance/client-apis/client.txt#2.4.2.1", expected_content])


def test_extract_annotation_block():
    """Test a valid annotation block in dafny format."""
    actual_anno = AnnotationParser([pathlib.Path("test.dfy")])._extract_annotation_block(
        TEST_DAFNY_STR.splitlines(keepends=True), 0, 5, pathlib.Path("test.dfy")
    )
    assert actual_anno.type.name == "IMPLICATION"
    assert actual_anno.target == "compliance/client-apis/client.txt#2.4.2.1"
    assert actual_anno.content == expected_content
    assert actual_anno.uri == "$".join(["compliance/client-apis/client.txt#2.4.2.1", expected_content])
