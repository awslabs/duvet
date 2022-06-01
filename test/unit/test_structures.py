# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Unit test suite for duvet.structures"""
import pytest
from duvet.identifiers import RequirementLevel, AnnotationType
from duvet.structures import Annotation

pytestmark = [pytest.mark.unit, pytest.mark.local]

def test_annotation():
    test_anno = Annotation(
        "test_target.md#target", AnnotationType.CITATION, "content", 1, 2, "test_target#target$content", "code.py"
    )
    assert test_anno.target == "test_target.md#target"
    assert test_anno.type == AnnotationType.CITATION
    assert test_anno.content == "content"
    assert test_anno.start_line == 1
    assert test_anno.end_line == 2
    assert test_anno.id == "test_target#target$content"
    assert test_anno.location == "code.py"
