# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Unit test suite for duvet.structures"""
import pytest

from duvet.identifiers import AnnotationType, RequirementLevel, RequirementStatus
from duvet.structures import Annotation, ExceptionAnnotation, Report, Requirement, Section, Specification

pytestmark = [pytest.mark.unit, pytest.mark.local]


def test_annotation():
    test_anno = Annotation(
        "test_target.md#target", AnnotationType.CITATION, "content", 1, 2, "test_target#target$content", "code.py"
    )
    assert test_anno.target == "test_target.md#target"
    assert test_anno.anno_type == AnnotationType.CITATION
    assert test_anno.content == "content"
    assert test_anno.start_line == 1
    assert test_anno.end_line == 2
    assert test_anno.uri == "test_target#target$content"
    assert test_anno.location == "code.py"


def test_requirement():
    test_anno = Annotation(
        "test_target.md#target", AnnotationType.CITATION, "content", 1, 2, "test_target#target$content", "code.py"
    )
    test_req = Requirement(RequirementLevel.MUST, "content", "test_target#target$content")
    test_req.add_annotation(test_anno)
    assert test_req.requirement_level == RequirementLevel.MUST
    assert test_req.status == RequirementStatus.MISSING_TEST
    assert test_req.content == "content"
    assert test_req.implemented
    assert not test_req.attested
    assert not test_req.omitted
    assert test_req.uri == "test_target#target$content"
    assert test_req.matched_annotations["test_target#target$content"] == test_anno


def test_add_annotation():
    citation_anno = Annotation(
        "test_target.md#target", AnnotationType.CITATION, "content", 1, 2, "test_target.md#target$content", "code.py"
    )
    test_anno = Annotation(
        "test_target.md#target", AnnotationType.TEST, "content", 1, 2, "test_target.md#target$content", "code.py"
    )
    test_req = Requirement(RequirementLevel.MUST, "content", "test_target.md#target$content")
    test_req.add_annotation(citation_anno)
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
        "content",
        "test_target#target$content",
    )
    assert not test_req.omitted
    test_req.add_annotation(exception_anno)
    assert test_req.omitted


def test_section():
    test_req = Requirement(
        RequirementLevel.MUST,
        "content",
        "test_target#target$content",
    )
    test_sec = Section("A Section Title", "h1.h2.h3.a-section-title", 1, 3)
    assert test_sec.title == "A Section Title"
    assert test_sec.uri == "h1.h2.h3.a-section-title"
    assert (
        test_sec.to_github_url("spec/spec.md", "https://github.com/awslabs/duvet")
        == "https://github.com/awslabs/duvet/blob/master/spec/spec.md#a-section-title"
    )
    assert not test_sec.has_requirements
    test_sec.add_requirement(test_req)
    assert test_sec.has_requirements


def test_specification():
    test_sec = Section("A Section Title", "h1.h2.h3.a-section-title", 1, 3)
    test_spec = Specification("A Specification Title", "spec/spec.md")
    test_spec.add_section(test_sec)
    assert (
        test_spec.to_github_url("https://github.com/awslabs/duvet")
        == "https://github.com/awslabs/duvet/blob/master/spec/spec.md"
    )


def test_report_add_annotation():
    test_rep = Report()
    # Verify the initialization of the report pass_fail
    assert not test_rep.pass_fail
    test_sec = Section("target", "target", 1, 3)
    test_spec = Specification("target", "test_target.md")
    test_spec.add_section(test_sec)
    test_req = Requirement(
        RequirementLevel.MUST,
        "content",
        "test_target.md#target$content",
    )
    test_sec.add_requirement(test_req)
    # Verify that the add_specification is correct
    test_rep.add_specification(test_spec)
    assert test_spec in test_rep.specifications.values()
    citation_anno = Annotation(
        "test_target.md#target", AnnotationType.CITATION, "content", 1, 2, "test_target.md#target$content", "code.py"
    )
    test_anno = Annotation(
        "test_target.md#target", AnnotationType.TEST, "content", 1, 2, "test_target.md#target$content", "code.py"
    )
    # Verify that the call chain is correct by checking against the requirement status
    test_rep.add_annotation(citation_anno)
    assert test_req.implemented
    assert not test_req.attested
    assert not test_req.omitted
    test_rep.add_annotation(test_anno)
    assert test_req.implemented
    assert test_req.attested


def test_exception_annotaion():
    test_anno = ExceptionAnnotation(
        "test_target.md#target", AnnotationType.EXCEPTION, "content", 1, 2, "test_target#target$content", "code.py"
    )
    test_anno.add_reason("reason")
    assert test_anno.target == "test_target.md#target"
    assert test_anno.anno_type == AnnotationType.EXCEPTION
    assert test_anno.content == "content"
    assert test_anno.reason == "reason"
    assert test_anno.start_line == 1
    assert test_anno.end_line == 2
    assert test_anno.uri == "test_target#target$content"
    assert test_anno.location == "code.py"
    assert test_anno.has_reason
