# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Functional test for Duvet Report Analysis."""
import copy

import pytest

from duvet.identifiers import AnnotationType, RequirementLevel
from duvet.structures import Annotation, Report, Requirement, Section
from duvet.summary import SummaryReport

pytestmark = [pytest.mark.local, pytest.mark.functional]

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

    def test_analyze_exception(self, under_test, tmp_path):
        # Requirement MUST be in summary if not completes.
        incomplete_table = copy.deepcopy(TABLE)
        incomplete_table[0][2], incomplete_table[0][3] = 1, 1
        self.actual_section.add_requirement(self.actual_requirement)
        exception_args = copy.deepcopy(ARGS)
        exception_args[1] = AnnotationType.EXCEPTION
        self.actual_section.add_annotation(Annotation(*exception_args))

        assert under_test.analyze_report()

    def test_analyze_exception_and_citation(self, under_test, tmp_path):
        # Requirement MUST be in summary if not completes.
        self.actual_section.add_requirement(self.actual_requirement)
        exception_args = copy.deepcopy(ARGS)
        exception_args[1] = AnnotationType.EXCEPTION
        self.actual_section.add_annotation(Annotation(*exception_args))
        self.actual_section.add_annotation(Annotation(*ARGS))

        assert under_test.analyze_report()
