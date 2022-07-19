# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Functional testing for requirement parsing"""

import pytest

from duvet.requirement_parser import RequirementParser

from ..utils import populate_file  # isort:skip

pytestmark = [pytest.mark.local, pytest.mark.functional]


def test_extract_duvet_specification(pytestconfig):
    path = pytestconfig.rootpath.joinpath("duvet-specification/compliance/duvet-specification.txt")
    actual_report = RequirementParser.process_specifications([path])
    actual_spec = actual_report.specifications.get('compliance/duvet-specification.txt')
    expected_title = "duvet-specification.txt"
    assert actual_spec.title == expected_title
    assert len(actual_spec.sections) == 30


VALID_RFC = """2.  Duvet specification

2.1.  Introduction

   Duvet is an application to build confidence that your software is
   correct. Sublists MUST be
   treated as if the parent item were terminated by the sublist.  List
   elements MAY contain a period (.) or exclamation point (!) and this
   punctuation MUST NOT terminate the requirement by excluding the
   following elements from the list of requirements.
"""


def test_valid_requirement_block(tmp_path):
    path = populate_file(tmp_path, VALID_RFC, "valid-md-spec.txt")
    actual_report = RequirementParser.process_specifications([path])
    actual_spec = actual_report.specifications.get(f'{path.parent.name}/valid-md-spec.txt')
    expected_title = "valid-md-spec.txt"
    assert actual_spec.title == expected_title
    # Expected 4, Duvet specification, Introduction, Specification, Section.
    assert len(actual_spec.sections) == 2
