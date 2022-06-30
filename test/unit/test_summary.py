# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Unit tests for ``duvet.summary``."""
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
def under_test(tmp_path) -> SummaryReport:
    return SummaryReport(Report())


class TestSummaryReport:
    actual_requirement = Requirement(RequirementLevel.MUST, "content", "test_target.md#target$content")
    actual_section = Section("A Section Title", "h1.h2.h3.a-section-title", 1, 3)

    def test_analyze_incomplete_stats(self, under_test, tmp_path):
        assert under_test._analyze_stats(self.actual_section) == TABLE

        # Requirement MUST be in summary if not completes.
        incomplete_table = copy.deepcopy(TABLE)
        incomplete_table[0][2], incomplete_table[0][3] = 1, 1
        self.actual_section.add_requirement(self.actual_requirement)
        assert under_test._analyze_stats(self.actual_section) == incomplete_table

    def test_analyze_complete_stats(self, under_test, tmp_path):
        # Requirement MUST be in marked complete if we have both implementation and test.
        citation_annotation = Annotation(*ARGS)
        test_args = copy.deepcopy(ARGS)
        test_args[1] = AnnotationType.TEST
        test_annotation = Annotation(*test_args)
        complete_table = copy.deepcopy(TABLE)
        complete_table[0][2] = 1
        self.actual_section.add_requirement(self.actual_requirement)
        self.actual_requirement.add_annotation(citation_annotation)
        self.actual_requirement.add_annotation(test_annotation)
        assert self.actual_requirement.analyze_annotations()
        assert under_test._analyze_stats(self.actual_section) == complete_table
