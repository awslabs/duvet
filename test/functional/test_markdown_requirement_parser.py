# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Functional testing for requirement parsing"""

import pytest

from duvet.markdown_requirement_parser import MarkdownRequirementParser

from .constants import REQUIREMENT_BLOCK  # isort: skip
from ..utils import populate_file  # isort: skip

pytestmark = [pytest.mark.local, pytest.mark.functional]


def test_extract_duvet_specification(pytestconfig):
    path = pytestconfig.rootpath.joinpath("duvet-specification", "duvet-specification.md")
    actual_spec = MarkdownRequirementParser.process_specifications([path]).specifications.get(
        f"{path.parent.name}/duvet-specification.md"
    )
    expected_title = "duvet-specification.md"

    assert actual_spec.title == expected_title



def test_valid_requirement_block(tmp_path):
    path = populate_file(tmp_path, REQUIREMENT_BLOCK, "valid-md-spec.md")
    actual_report = MarkdownRequirementParser.process_specifications([path])
    actual_spec = actual_report.specifications.get(f"{path.parent.name}/valid-md-spec.md")
    expected_title = "valid-md-spec.md"
    assert actual_spec.title == expected_title
    # Expected 4, Duvet specification, Introduction, Specification, Section.
    assert len(actual_spec.sections) == 4
