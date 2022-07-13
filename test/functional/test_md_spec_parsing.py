# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Functional testing for requirement parsing"""

import pytest

from duvet.spec_md_parser import MDSpec

from ..utils import populate_file  # isort:skip

pytestmark = [pytest.mark.local, pytest.mark.functional]

REQUIREMENT_BLOCK = """# Duvet specification

## Introduction

Duvet is an application to build confidence that your software is correct.

## Specification

A specification is a document, like this, that defines correct behavior.
This behavior is defined in regular human language.

### Section

The top level header for requirements is the name of a section.
The name of the sections MUST NOT be nested.
A requirements section MUST be the top level containing header.
A header MUST NOT itself be a requirement.
"""


def test_extract_duvet_specification(pytestconfig):
    path = pytestconfig.rootpath.joinpath("duvet-specification", "duvet-specification.md")
    actual_spec = MDSpec.load(path).get_spec()
    expected_title = "duvet-specification.md"
    assert actual_spec.title == expected_title
    # print(new_spec.get_spec())
    # print(new_spec)


def test_valid_requirement_block(tmp_path):
    path = populate_file(tmp_path, REQUIREMENT_BLOCK, "valid-md-pec.md")
    new_spec = MDSpec.load(path)
    actual_spec = new_spec.get_spec()
    expected_title = "valid-md-pec.md"
    assert actual_spec.title == expected_title
    # Expected 4, Duvet specification, Introduction, Specification, Section.
    assert len(actual_spec.sections) == 4
