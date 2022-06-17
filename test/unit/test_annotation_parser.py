# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Annotation Parser used by duvet-python."""
import pytest

from duvet.annotation_parser import AnnotationParser
from src.duvet.identifiers import *
from src.duvet.annotation_parser import *

pytestmark = [pytest.mark.unit, pytest.mark.local]

TEST_DAFNY_STR = (
    "//= compliance/client-apis/client.txt#2.4.2.1\n"
    "//= type=implication\n"
    "//# * encrypt (encrypt.md) MUST only support algorithm suites that have\n"
    "//# a Key Commitment (../framework/algorithm-suites.md#algorithm-\n"
    "//# suites-encryption-key-derivation-settings) value of False\n"
)

TEST_PYTHON_STR = (
    "//= compliance/client-apis/client.txt#2.4.2.1\n"
    "//= type=implication\n"
    "//# * encrypt (encrypt.md) MUST only support algorithm suites that have\n"
    "//# a Key Commitment (../framework/algorithm-suites.md#algorithm-\n"
    "//# suites-encryption-key-derivation-settings) value of False\n"
)


def test_extract_annotation():
    temp_anno = AnnotationParser()._extract_annotation(TEST_DAFNY_STR, 0, )
    assert temp_anno.type.name == "IMPLICATION"
    assert temp_anno.target == 'compliance/client-apis/client.txt#2.4.2.1'
    assert temp_anno.content == (
        '* encrypt (encrypt.md) MUST only support algorithm suites that havea Key '
        'Commitment '
        '(../framework/algorithm-suites.md#algorithm-suites-encryption-key-derivation-settings) '
        'value of False')
