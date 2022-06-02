# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Unit test suite for duvet.structures"""
import pytest

from duvet.identifiers import AnnotationType, RequirementLevel, RequirementStatus
from duvet.structures import Annotation, Requirement

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


def test_requirement():
    test_anno = Annotation(
        "test_target.md#target", AnnotationType.CITATION, "content", 1, 2, "test_target#target$content", "code.py"
    )
    test_req = Requirement(
        RequirementLevel.MUST,
        RequirementStatus.NOT_STARTED,
        False,
        False,
        False,
        "content",
        "test_target#target$content",
        {"test_target#target$content": test_anno},
    )

    assert test_req.requirement_level == RequirementLevel.MUST
    assert test_req.status == RequirementStatus.MISSING_TEST
    assert test_req.content == "content"
    assert test_req.implemented
    assert not test_req.attested
    assert not test_req.omitted
    assert test_req.id == "test_target#target$content"
    assert test_req.matched_annotations["test_target#target$content"] == test_anno


def test_add_annotation():
    citation_anno = Annotation(
        "test_target.md#target", AnnotationType.CITATION, "content", 1, 2, "test_target#target$content", "code.py"
    )
    test_anno = Annotation(
        "test_target.md#target", AnnotationType.TEST, "content", 1, 2, "test_target#target$content", "code.py"
    )
    test_req = Requirement(
        RequirementLevel.MUST,
        RequirementStatus.NOT_STARTED,
        False,
        False,
        False,
        "content",
        "test_target#target$content",
        {"test_target#target$content": citation_anno},
    )
    assert test_req.implemented
    assert not test_req.attested
    assert not test_req.omitted
    test_req.add_annotation(test_anno)
    assert test_req.implemented
    assert test_req.attested


def test_add_excepted_annotation():
    exception_anno = Annotation(
        "test_target.md#target", AnnotationType.EXCEPTION, "content", 1, 2, "test_target#target$content", "code.py"
    )
    test_req = Requirement(
        RequirementLevel.MUST,
        RequirementStatus.NOT_STARTED,
        False,
        False,
        False,
        "content",
        "test_target#target$content",
        {},
    )
    assert not test_req.omitted
    test_req.add_annotation(exception_anno)
    assert test_req.omitted
