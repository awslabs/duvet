# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Unit tests for Duvet.summary."""
import copy

import pytest

from duvet.identifiers import AnnotationType, RequirementLevel
from duvet.structures import Annotation, Report, Requirement, Section
from duvet.summary import SummaryReport

pytestmark = [pytest.mark.local, pytest.mark.unit]

TABLE = [
    ["h1.h2.h3.a-section-title", "MUST", 0, 0],
    ["h1.h2.h3.a-section-title", "SHOULD", 0, 0],
    ["h1.h2.h3.a-section-title", "MAY", 0, 0],
]

ARGS = ["test_target.md#target", AnnotationType.CITATION, "content", 1, 2, "test_target.md#target$content", "code.py"]


@pytest.fixture
def under_test() -> SummaryReport:
    return SummaryReport(Report())


@pytest.fixture
def actual_requirement() -> Requirement:
    return Requirement(RequirementLevel.MUST, "content", "test_target.md#target$content")


@pytest.fixture
def actual_section() -> Section:
    return Section("A Section Title", "h1.h2.h3.a-section-title", 1, 3)


class TestSummaryReport:
    def test_analyze_incomplete_stats(self, under_test, tmp_path, actual_section, actual_requirement):
        assert under_test.analyze_stats(actual_section) == TABLE

        # Requirement MUST be in summary if not completes.
        incomplete_table = copy.deepcopy(TABLE)
        incomplete_table[0][2], incomplete_table[0][3] = 1, 1
        actual_section.add_requirement(actual_requirement)
        assert under_test.analyze_stats(actual_section) == incomplete_table

    def test_analyze_complete_stats(self, under_test, tmp_path, actual_requirement, actual_section):
        # Requirement MUST be in marked complete if we have both implementation and test.
        citation_annotation = Annotation(*ARGS)
        test_args = copy.deepcopy(ARGS)
        test_args[1] = AnnotationType.TEST
        test_annotation = Annotation(*test_args)
        complete_table = copy.deepcopy(TABLE)
        complete_table[0][2] = 1
        actual_section.add_requirement(actual_requirement)
        actual_requirement.add_annotation(citation_annotation)
        actual_requirement.add_annotation(test_annotation)
        assert actual_requirement.analyze_annotations()
        assert under_test.analyze_stats(actual_section) == complete_table
