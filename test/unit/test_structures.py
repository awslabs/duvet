# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Unit test suite for duvet.structures"""
import copy
import logging

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
    "source": "code.py",
}


def _update_valid_kwargs(updates: dict) -> dict:
    rtn = copy.deepcopy(VALID_KWARGS)
    rtn.update(updates)
    return rtn


INVALID_KWARGS = _update_valid_kwargs(
    {"target": "new_test_target.md#new-target", "uri": "new_test_target.md#target$content"}
)


def _help_assert_annotation(annotation: Annotation, kwargs: dict):
    assert annotation.target == kwargs["target"]
    assert annotation.type == kwargs["type"]
    assert annotation.content == kwargs["content"]
    assert annotation.start_line == kwargs["start_line"]
    assert annotation.end_line == kwargs["end_line"]
    assert annotation.uri == kwargs["uri"]
    assert annotation.source == kwargs["source"]


@pytest.fixture
def actual_requirement() -> Requirement:
    return Requirement(RequirementLevel.MUST, "content", "test_target.md#target$content")


@pytest.fixture
def actual_specification() -> Specification:
    return Specification("target", "test_target.md")


@pytest.fixture
def actual_report() -> Report:
    return Report()


@pytest.fixture
def citation() -> Annotation:
    return Annotation(**VALID_KWARGS)


@pytest.fixture
def actual_section() -> Section:
    return Section("target", "test_target.md#target", 1, 3)


class TestRequirement:
    def test_requirement(self, actual_requirement, citation):
        actual_requirement.add_annotation(citation)
        assert actual_requirement.requirement_level == RequirementLevel.MUST
        assert actual_requirement.status == RequirementStatus.NOT_STARTED
        assert actual_requirement.content == "content"
        assert actual_requirement.uri == "test_target.md#target$content"
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
        assert not actual_requirement.analyze_annotations()
        assert actual_requirement.implemented
        actual_requirement.add_annotation(test_annotation)
        assert actual_requirement.analyze_annotations()
        assert actual_requirement.implemented
        assert actual_requirement.attested

    def test_add_excepted_annotation(self, actual_requirement):
        exception_annotation = Annotation(**_update_valid_kwargs({"type": AnnotationType.EXCEPTION}))
        actual_requirement.add_annotation(exception_annotation)
        assert exception_annotation in actual_requirement.matched_annotations
        assert not actual_requirement.analyze_annotations()

    def test_exception_annotation_and_add_reason(self, actual_requirement):
        exception_kwargs = _update_valid_kwargs({"type": AnnotationType.EXCEPTION, "reason": "reason"})
        reasoned_exception = Annotation(**exception_kwargs)
        _help_assert_annotation(reasoned_exception, exception_kwargs)
        assert reasoned_exception.has_reason()

        # Verify reason added in the exception.
        actual_requirement.add_annotation(reasoned_exception)
        assert reasoned_exception in actual_requirement.matched_annotations
        assert actual_requirement.analyze_annotations()


class TestSection:
    def test_create_section_and_add_requirements(self, actual_section, actual_requirement):
        assert actual_section.title == "target"
        assert actual_section.uri == "test_target.md#target"
        github_url = actual_section.to_github_url("test_target.md", "https://github.com/awslabs/duvet")
        assert github_url == "https://github.com/awslabs/duvet/blob/master/test_target.md#target"

        # Verify the logic of has requirements and add requirement
        assert not actual_section.has_requirements
        actual_section.add_requirement(actual_requirement)
        assert actual_section.has_requirements

    def test_section_add_citation(self, actual_section, citation, actual_requirement):
        # Set up section.
        actual_section.add_requirement(actual_requirement)

        # Add citation
        assert actual_section.add_annotation(citation)

        # Prove citation added.
        assert citation in actual_requirement.matched_annotations

    # def test_section_add_mismatch_annotation(self, actual_section, actual_requirement, caplog):
    #     caplog.set_level(logging.INFO)
    #
    #     # Set up section
    #     actual_section.add_requirement(actual_requirement)
    #
    #     # Try to add mismatched annotation to section
    #     mismatch_citation = Annotation(**INVALID_KWARGS)
    #     assert not actual_section.add_annotation(mismatch_citation)
    #
    #     # Check log information.
    #     assert f"{mismatch_citation.uri} not found in {actual_section.uri}" in caplog.text


class TestSpecification:
    def test_specification(self, actual_specification, actual_section):
        actual_specification.add_section(actual_section)
        github_url = actual_specification.to_github_url("https://github.com/awslabs/duvet")
        assert github_url == "https://github.com/awslabs/duvet/blob/master/test_target.md"

    def test_specification_add_citation(
        self, actual_specification, actual_section, actual_requirement, citation, caplog
    ):
        # Set up specification
        actual_section.add_requirement(actual_requirement)
        actual_specification.add_section(actual_section)

        # Try to add citation to specification
        assert actual_specification.add_annotation(citation)

        # Verify the citation successfully added
        assert citation in actual_requirement.matched_annotations

    def test_specification_add_invalid_annotation(
        self, actual_specification, actual_section, actual_requirement, caplog
    ):
        caplog.set_level(logging.INFO)

        # Set up specification
        actual_section.add_requirement(actual_requirement)
        actual_specification.add_section(actual_section)

        # Try to add mismatched annotation to specification.
        mismatch_citation = Annotation(**INVALID_KWARGS)
        assert not actual_specification.add_annotation(mismatch_citation)

        # Prove no citation added.
        assert len(actual_requirement.matched_annotations) == 0

        # Check log information.
        assert f"{mismatch_citation.target} not found in {actual_specification.source}" in caplog.text


class TestReport:
    def test_create_report_and_analyze_annotations(
        self, actual_report, actual_section, actual_specification, actual_requirement, citation
    ):
        # Set up for specification
        actual_specification.add_section(actual_section)
        actual_section.add_requirement(actual_requirement)

        # Verify that the add_specification is correct
        actual_report.add_specification(actual_specification)
        assert actual_specification in actual_report.specifications.values()

        test_kwargs = _update_valid_kwargs({"type": AnnotationType.TEST})
        test_annotation = Annotation(**test_kwargs)
        actual_report.add_annotation(citation)

        # Verify that the call chain is correct by checking the citation in requirement
        assert citation in actual_requirement.matched_annotations
        assert not actual_report.analyze_annotations()
        assert actual_requirement.status == RequirementStatus.MISSING_PROOF

        # Verify that the call chain is correct by checking test annotation in requirement
        actual_report.add_annotation(test_annotation)
        assert test_annotation in actual_requirement.matched_annotations
        assert actual_report.analyze_annotations()

    def test_report_add_mismatch_annotation(
        self, actual_report, actual_section, actual_specification, actual_requirement, caplog
    ):
        caplog.set_level(logging.INFO)

        # Set up for report.
        actual_specification.add_section(actual_section)
        actual_section.add_requirement(actual_requirement)
        actual_report.add_specification(actual_specification)

        # Try to add mismatched annotation to report.
        mismatch_citation = Annotation(**INVALID_KWARGS)
        assert not actual_report.add_annotation(mismatch_citation)

        # Prove no citation added.
        assert len(actual_requirement.matched_annotations) == 0

        # Check log information.
        spec_id = mismatch_citation.target.split("#")[0]
        assert f"{spec_id} not found in report" in caplog.text
