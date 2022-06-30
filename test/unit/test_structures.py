# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Unit test suite for duvet.structures"""
import pytest

from duvet.identifiers import AnnotationType, RequirementLevel, RequirementStatus
from duvet.structures import Annotation, Report, Requirement, Section, Specification

pytestmark = [pytest.mark.unit, pytest.mark.local]


class TestRequirement:
    def test_requirement(self):
        actual_annotation = Annotation(
            "test_target.md#target", AnnotationType.CITATION, "content", 1, 2, "test_target#target$content", "code.py"
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
    def test_annotation(self):
        actual_annotation = Annotation(
            "test_target.md#target", AnnotationType.CITATION, "content", 1, 2, "test_target#target$content", "code.py"
        )
        assert actual_annotation.target == "test_target.md#target"
        assert actual_annotation.type == AnnotationType.CITATION
        assert actual_annotation.content == "content"
        assert actual_annotation.start_line == 1
        assert actual_annotation.end_line == 2
        assert actual_annotation.uri == "test_target#target$content"
        assert actual_annotation.location == "code.py"

    def test_add_annotation(self):
        citation_anno = Annotation(
            "test_target.md#target",
            AnnotationType.CITATION,
            "content",
            1,
            2,
            "test_target.md#target$content",
            "code.py",
        )
        actual_annotation = Annotation(
            "test_target.md#target", AnnotationType.TEST, "content", 1, 2, "test_target.md#target$content", "code.py"
        )
        actual_requirement = Requirement(RequirementLevel.MUST, "content", "test_target.md#target$content")
        actual_requirement.add_annotation(citation_anno)
        actual_requirement.analyze_annotations()
        assert actual_requirement.implemented
        assert not actual_requirement.attested
        actual_requirement.add_annotation(actual_annotation)
        actual_requirement.analyze_annotations()
        assert actual_requirement.implemented
        assert actual_requirement.attested

    def test_add_excepted_annotation(self):
        exception_anno = Annotation(
            "test_target.md#target", AnnotationType.EXCEPTION, "content", 1, 2, "test_target#target$content", "code.py"
        )
        actual_requirement = Requirement(
            RequirementLevel.MUST,
            "content",
            "test_target#target$content",
        )
        actual_requirement.add_annotation(exception_anno)
        actual_requirement.analyze_annotations()

    def test_exception_annotation(self):
        actual_annotation = Annotation(
            "test_target.md#target",
            AnnotationType.EXCEPTION,
            "content",
            1,
            2,
            "test_target#target$content",
            "code.py",
            "reason",
        )
        assert actual_annotation.target == "test_target.md#target"
        assert actual_annotation.type == AnnotationType.EXCEPTION
        assert actual_annotation.content == "content"
        assert actual_annotation.reason == "reason"
        assert actual_annotation.start_line == 1
        assert actual_annotation.end_line == 2
        assert actual_annotation.uri == "test_target#target$content"
        assert actual_annotation.location == "code.py"
        assert actual_annotation.has_reason()


class TestSection:
    def test_create_section_and_add_requirements(self):
        actual_requirement = Requirement(
            RequirementLevel.MUST,
            "content",
            "test_target#target$content",
        )
        actual_section = Section("A Section Title", "h1.h2.h3.a-section-title", 1, 3)
        assert actual_section.title == "A Section Title"
        assert actual_section.uri == "h1.h2.h3.a-section-title"
        assert (
            actual_section.to_github_url("spec/spec.md", "https://github.com/awslabs/duvet")
            == "https://github.com/awslabs/duvet/blob/master/spec/spec.md#a-section-title"
        )

        # Verify the logic of has requirements and add requirement
        assert not actual_section.has_requirements
        actual_section.add_requirement(actual_requirement)
        assert actual_section.has_requirements


class TestSpecification:
    def test_specification(self):
        actual_section = Section("A Section Title", "h1.h2.h3.a-section-title", 1, 3)
        actual_specification = Specification("A Specification Title", "spec/spec.md")
        actual_specification.add_section(actual_section)
        assert (
            actual_specification.to_github_url("https://github.com/awslabs/duvet")
            == "https://github.com/awslabs/duvet/blob/master/spec/spec.md"
        )


class TestReport:
    def test_create_report_and_analyze_annotations(self):
        actual_report = Report()
        # Verify the initialization of the report pass_fail
        assert not actual_report.report_pass
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
        actual_report.add_specification(actual_specification)
        assert actual_specification in actual_report.specifications.values()

        citation_annotation = Annotation(
            "test_target.md#target",
            AnnotationType.CITATION,
            "content",
            1,
            2,
            "test_target.md#target$content",
            "code.py",
        )
        actual_annotation = Annotation(
            "test_target.md#target", AnnotationType.TEST, "content", 1, 2, "test_target.md#target$content", "test.py"
        )
        actual_report.add_annotation(citation_annotation)

        # Verify that the call chain is correct by checking against the requirement status
        assert not actual_report.analyze_annotations()

        # Verify that the call chain is correct by checking against the requirement status
        actual_report.add_annotation(actual_annotation)
        assert actual_requirement.analyze_annotations()
