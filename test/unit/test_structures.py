# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Unit test suite for duvet.structures"""
import copy
from typing import Optional, Union

import pytest

from duvet.identifiers import AnnotationType, RequirementLevel, RequirementStatus
from duvet.structures import Annotation, Report, Requirement, Section, Specification

pytestmark = [pytest.mark.unit, pytest.mark.local]

ARGS = [
    "new_test_target.md#target",
    AnnotationType.TEST,
    "content",
    1,
    2,
    "new_test_target.md#target$content",
    "test.py",
]

VALID_KWARGS = {
    "target": "test_target.md#target",
    "type": AnnotationType.CITATION,
    "start_line": 1,
    "end_line": 2,
    "reason": None,
    "content": "content",
    "uri":"test_target.md#target$content",
    "location":"code.py"
}

def _update_valid_kwargs(updates: dict) -> dict:
    rtn = copy.deepcopy(VALID_KWARGS)
    rtn.update(updates)
    return rtn


class TestRequirement:
    def test_requirement(self):
        actual_annotation = Annotation(
            **VALID_KWARGS
        )
        actual_requirement = Requirement(RequirementLevel.MUST, "content", "test_target#target$content")
        actual_requirement.add_annotation(actual_annotation)
        assert actual_requirement.requirement_level == RequirementLevel.MUST
        assert actual_requirement.status == RequirementStatus.NOT_STARTED
        assert actual_requirement.content == "content"
        assert actual_requirement.uri == "test_target#target$content"
        assert len(actual_requirement.matched_annotations) == 1

        # Verify requirement will not pass the analysis
        assert not actual_requirement.analyze_annotations()
        assert actual_requirement.implemented


class TestAnnotation:
    actual_requirement = Requirement(RequirementLevel.MUST, "content", "test_target.md#target$content")
    citation = Annotation(**VALID_KWARGS)

    def test_annotation(self):
        assert self.citation.target == "test_target.md#target"
        assert self.citation.type == AnnotationType.CITATION
        assert self.citation.content == "content"
        assert self.citation.start_line == 1
        assert self.citation.end_line == 2
        assert self.citation.uri == "test_target.md#target$content"
        assert self.citation.location == "code.py"

    def test_add_annotation(self):
        test_args = copy.deepcopy(**VALID_KWARGS)
        test_args[1] = AnnotationType.TEST
        actual_annotation = Annotation(*test_args)
        self.actual_requirement.add_annotation(self.citation_anno)
        self.actual_requirement.analyze_annotations()
        assert self.actual_requirement.implemented
        self.actual_requirement.add_annotation(actual_annotation)
        self.actual_requirement.analyze_annotations()
        assert self.actual_requirement.implemented
        assert self.actual_requirement.attested

    def test_add_excepted_annotation(self):
        exception_args = copy.deepcopy(**VALID_KWARGS)
        exception_args[1] = AnnotationType.EXCEPTION
        exception_anno = Annotation(*exception_args)
        actual_requirement = Requirement(
            RequirementLevel.MUST,
            "content",
            "test_target#target$content",
        )
        actual_requirement.add_annotation(exception_anno)
        actual_requirement.analyze_annotations()

    def test_exception_annotation(self):
        exception_args = copy.deepcopy(**VALID_KWARGS)
        exception_args[1] = AnnotationType.EXCEPTION
        exception_args.append("reason")
        actual_annotation = Annotation(*exception_args)
        assert actual_annotation.target == "test_target.md#target"
        assert actual_annotation.type == AnnotationType.EXCEPTION
        assert actual_annotation.content == "content"
        assert actual_annotation.reason == "reason"
        assert actual_annotation.start_line == 1
        assert actual_annotation.end_line == 2
        assert actual_annotation.uri == "test_target.md#target$content"
        assert actual_annotation.location == "code.py"
        assert actual_annotation.has_reason()

        self.actual_requirement.add_annotation(actual_annotation)
        self.actual_requirement.analyze_annotations()
        assert actual_annotation.has_reason()


class TestSection:
    actual_requirement = Requirement(
        RequirementLevel.MUST,
        "content",
        "test_target#target$content",
    )
    actual_section = Section("A Section Title", "h1.h2.h3.a-section-title", 1, 3)

    def test_create_section_and_add_requirements(self):
        assert self.actual_section.title == "A Section Title"
        assert self.actual_section.uri == "h1.h2.h3.a-section-title"
        github_url = self.actual_section.to_github_url("spec/spec.md", "https://github.com/awslabs/duvet")
        assert github_url == "https://github.com/awslabs/duvet/blob/master/spec/spec.md#a-section-title"

        # Verify the logic of has requirements and add requirement
        assert not self.actual_section.has_requirements
        self.actual_section.add_requirement(self.actual_requirement)
        assert self.actual_section.has_requirements

    def test_specification_add_invalid_annotation(self):
        assert not self.actual_section.add_annotation(Annotation(*ARGS))


class TestSpecification:
    actual_specification = Specification("A Specification Title", "spec/spec.md")

    def test_specification(self):
        actual_section = Section("A Section Title", "h1.h2.h3.a-section-title", 1, 3)
        self.actual_specification.add_section(actual_section)
        github_url = self.actual_specification.to_github_url("https://github.com/awslabs/duvet")
        assert github_url == "https://github.com/awslabs/duvet/blob/master/spec/spec.md"

    def test_specification_add_invalid_annotation(self):
        assert not self.actual_specification.add_annotation(Annotation(*ARGS))


class TestReport:
    actual_report = Report()

    def test_create_report_and_analyze_annotations(self):
        # Verify the initialization of the report pass_fail
        assert not self.actual_report.report_pass
        actual_section = Section("target", "target", 1, 3)
        actual_specification = Specification("target", "test_target.md")

        actual_specification.add_section(actual_section)
        actual_requirement = Requirement(
            RequirementLevel.MUST,
            "content",
            "test_target.md#target$content",
        )
        actual_section.add_requirement(actual_requirement)

        # Verify that the add_specification is correct
        self.actual_report.add_specification(actual_specification)
        assert actual_specification in self.actual_report.specifications.values()

        citation_annotation = Annotation(**VALID_KWARGS)
        actual_annotation = Annotation(
            "test_target.md#target", AnnotationType.TEST, "content", 1, 2, "test_target.md#target$content", "test.py"
        )
        self.actual_report.add_annotation(citation_annotation)

        # Verify that the call chain is correct by checking against the requirement status
        assert not self.actual_report.analyze_annotations()

        # Verify that the call chain is correct by checking against the requirement status
        self.actual_report.add_annotation(actual_annotation)
        assert actual_requirement.analyze_annotations()

    def test_report_add_invalid_annotation(self):
        assert not self.actual_report.add_annotation(Annotation(*ARGS))
