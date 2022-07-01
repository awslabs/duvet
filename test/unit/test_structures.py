# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Unit test suite for duvet.structures"""
import copy

import pytest

from duvet.identifiers import AnnotationType, RequirementLevel, RequirementStatus
from duvet.structures import Annotation, Report, Requirement, Section, Specification

pytestmark = [pytest.mark.unit, pytest.mark.local]

VALID_KWARGS: dict = {
    "target": "test_target.md#target",
    "type": AnnotationType.CITATION,
    "start_line": 1,
    "end_line": 2,
    "reason": None,
    "content": "content",
    "uri": "test_target.md#target$content",
    "location": "code.py",
}


def _update_valid_kwargs(updates: dict) -> dict:
    rtn = copy.deepcopy(VALID_KWARGS)
    rtn.update(updates)
    return rtn


INVALID_KWARGS = _update_valid_kwargs(
    {"target": "new_test_target.md#target", "uri": "new_test_target.md#target$content"}
)


def _help_assert_annotation(annotation: Annotation, kwargs: dict):
    assert annotation.target == kwargs["target"]
    assert annotation.type == kwargs["type"]
    assert annotation.content == kwargs["content"]
    assert annotation.start_line == kwargs["start_line"]
    assert annotation.end_line == kwargs["end_line"]
    assert annotation.uri == kwargs["uri"]
    assert annotation.location == kwargs["location"]


@pytest.fixture
def actual_requirement() -> Requirement:
    return Requirement(RequirementLevel.MUST, "content", "test_target#target$content")


@pytest.fixture
def actual_specification() -> Specification:
    return Specification("A Specification Title", "spec/spec.md")


@pytest.fixture
def actual_report() -> Report:
    return Report()


@pytest.fixture
def citation() -> Annotation:
    return Annotation(**VALID_KWARGS)


@pytest.fixture
def actual_section() -> Section:
    return Section("A Section Title", "h1.h2.h3.a-section-title", 1, 3)


class TestRequirement:
    def test_requirement(self, actual_requirement, citation):
        actual_requirement.add_annotation(citation)
        assert actual_requirement.requirement_level == RequirementLevel.MUST
        assert actual_requirement.status == RequirementStatus.NOT_STARTED
        assert actual_requirement.content == "content"
        assert actual_requirement.uri == "test_target#target$content"
        assert len(actual_requirement.matched_annotations) == 1

        # Verify requirement will not pass the analysis
        assert not actual_requirement.analyze_annotations()
        # Verify set_label
        assert actual_requirement.implemented


class TestAnnotation:
    def test_annotation(self, citation):
        _help_assert_annotation(citation, VALID_KWARGS)

    def test_add_annotation(self, actual_requirement, citation):
        test_annotation = Annotation(**_update_valid_kwargs({"type": AnnotationType.TEST}))
        actual_requirement.add_annotation(citation)
        actual_requirement.analyze_annotations()
        assert actual_requirement.implemented
        actual_requirement.add_annotation(test_annotation)
        actual_requirement.analyze_annotations()
        assert actual_requirement.implemented
        assert actual_requirement.attested

    def test_add_excepted_annotation(self):
        exception_annotation = Annotation(**_update_valid_kwargs({"type": AnnotationType.EXCEPTION}))
        actual_requirement = Requirement(
            RequirementLevel.MUST,
            "content",
            "test_target#target$content",
        )
        actual_requirement.add_annotation(exception_annotation)
        actual_requirement.analyze_annotations()

    def test_exception_annotation_and_add_reason(self, actual_requirement):
        exception_kwargs = _update_valid_kwargs({"type": AnnotationType.EXCEPTION, "reason": "reason"})
        actual_annotation = Annotation(**exception_kwargs)
        _help_assert_annotation(actual_annotation, exception_kwargs)
        assert actual_annotation.has_reason()

        # Verify reason added in the exception.
        actual_requirement.add_annotation(actual_annotation)
        actual_requirement.analyze_annotations()
        assert actual_annotation.has_reason()


class TestSection:
    def test_create_section_and_add_requirements(self, actual_section, actual_requirement):
        assert actual_section.title == "A Section Title"
        assert actual_section.uri == "h1.h2.h3.a-section-title"
        github_url = actual_section.to_github_url("spec/spec.md", "https://github.com/awslabs/duvet")
        assert github_url == "https://github.com/awslabs/duvet/blob/master/spec/spec.md#a-section-title"

        # Verify the logic of has requirements and add requirement
        assert not actual_section.has_requirements
        actual_section.add_requirement(actual_requirement)
        assert actual_section.has_requirements

    def test_specification_add_invalid_annotation(self, actual_section):
        assert not actual_section.add_annotation(Annotation(**INVALID_KWARGS))


class TestSpecification:
    def test_specification(self, actual_specification):
        actual_section = Section("A Section Title", "h1.h2.h3.a-section-title", 1, 3)
        actual_specification.add_section(actual_section)
        github_url = actual_specification.to_github_url("https://github.com/awslabs/duvet")
        assert github_url == "https://github.com/awslabs/duvet/blob/master/spec/spec.md"

    def test_specification_add_invalid_annotation(self, actual_specification):
        assert not actual_specification.add_annotation(Annotation(**INVALID_KWARGS))


class TestReport:
    def test_create_report_and_analyze_annotations(self, actual_report, citation):
        # Verify the initialization of the report pass or fail
        assert not actual_report.report_pass
        actual_section = Section("target", "test_target.md#target", 1, 3)
        actual_specification = Specification("target", "test_target.md")

        actual_specification.add_section(actual_section)
        actual_requirement = Requirement(
            RequirementLevel.MUST,
            "content",
            "test_target.md#target$content",
        )
        actual_section.add_requirement(actual_requirement)

        # Verify that the add_specification is correct
        actual_report.add_specification(actual_specification)
        assert actual_specification in actual_report.specifications.values()

        test_kwargs = _update_valid_kwargs({"type": AnnotationType.TEST})
        test_annotation = Annotation(**test_kwargs)
        actual_report.add_annotation(citation)

        # Verify that the call chain is correct by checking against the requirement status
        assert not actual_report.analyze_annotations()

        # Verify that the call chain is correct by checking against the requirement status
        actual_report.add_annotation(test_annotation)
        assert actual_report.analyze_annotations()

    def test_report_add_invalid_annotation(self, actual_report):
        assert not actual_report.add_annotation(Annotation(**INVALID_KWARGS))
