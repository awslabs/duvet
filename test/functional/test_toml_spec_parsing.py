# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Specification Parser used by duvet-python for toml format."""
import pathlib

import pytest

from duvet.spec_toml_parser import TomlRequirementParser

from ..utils import populate_file  # isort: skip

pytestmark = [pytest.mark.local, pytest.mark.functional]

TEST_SPEC_TOML_TARGET = """target = "../duvet-python/spec/spec.txt#2.2.1"    """

TEST_SPEC_TOML_COMMENT = """
# 2.2.1.  Section
#
# The top level header for requirements is the name of a section.  The
# name of the sections MUST NOT be nested.  A requirements section MUST
# be the top level containing header.  A header MUST NOT itself be a
# requirement.
# A section MUST be indexable by combining different levels of naming.
# This means that Duvet needs to be able to locate it uniquely within a
# specification.  A good example of a section is a header in an HTML or
# Markdown document.
"""

TEST_SPEC_TOML_SPEC = """
[[spec]]
level = "MUST"
quote = '''
The
name of the sections MUST NOT be nested.
'''

[[spec]]
level = "MUST"
quote = '''
A requirements section MUST
be the top level containing header.
'''

[[spec]]
level = "MUST"
quote = '''
A header MUST NOT itself be a
requirement.
'''

[[spec]]
level = "MUST"
quote = '''
A section MUST be indexable by combining different levels of naming.
'''
"""


def test_dogfood(pytestconfig):
    filepath = pytestconfig.rootpath.joinpath("duvet-specification")
    patterns = filepath.glob("compliance/**/*.toml")
    test_report = TomlRequirementParser().extract_toml_specs(patterns, filepath)
    # Verify one spec is added to the report object
    assert len(test_report.specifications.keys()) == 1


def test_missing_uri(tmp_path):
    # We will not throw error is there is no targset.
    patterns = tmp_path.glob("compliance/**/*.toml")
    populate_file(tmp_path, TEST_SPEC_TOML_COMMENT, "compliance/spec/section1.toml")
    with pytest.warns(UserWarning) as record:
        TomlRequirementParser().extract_toml_specs(patterns, tmp_path)
    # check that only one warning was raised
    assert len(record) == 1
    # check that the message matches
    assert (
        record[0].message.args[0]
        == f'{tmp_path}/compliance/spec/section1.toml: The key "target" is missing. Skipping file.'
    )


def test_missing_specs(tmp_path):
    # We will not throw error if there is no requirements.
    patterns = tmp_path.glob("compliance/**/*.toml")
    populate_file(tmp_path, TEST_SPEC_TOML_TARGET, "compliance/spec/section1.toml")
    actual_report = TomlRequirementParser().extract_toml_specs(patterns, tmp_path)
    actual_specifications = actual_report.specifications
    actual_specification = actual_specifications.get("../duvet-python/spec/spec.txt")
    assert actual_specification.title == "spec.txt"
    assert actual_specification.source == "../duvet-python/spec/spec.txt"
    # Verify one section is added to the report object
    assert (
        actual_specifications.get("../duvet-python/spec/spec.txt")
        .sections.get("../duvet-python/spec/spec.txt#2.2.1")
        .title
        == "2.2.1"
    )
    assert (
        actual_specifications.get("../duvet-python/spec/spec.txt")
        .sections.get("../duvet-python/spec/spec.txt#2.2.1")
        .uri
        == "../duvet-python/spec/spec.txt#2.2.1"
    )


def test_extract_spec_toml(tmp_path):
    # We will not throw error is there is no requirements.
    patterns = tmp_path.glob("compliance/**/*.toml")
    populate_file(
        tmp_path,
        "\n".join([TEST_SPEC_TOML_COMMENT, TEST_SPEC_TOML_TARGET, TEST_SPEC_TOML_SPEC]),
        "compliance/spec/section1.toml",
    )
    actual_report = TomlRequirementParser().extract_toml_specs(patterns, tmp_path)
    # Verify requirements is added to the report object
    actual_requirements = (
        actual_report.specifications.get("../duvet-python/spec/spec.txt")
        .sections.get("../duvet-python/spec/spec.txt#2.2.1")
        .requirements
    )
    assert (
        actual_requirements.get(
            "../duvet-python/spec/spec.txt#2.2.1$The name of the sections MUST NOT be nested."
        ).content
        == "The name of the sections MUST NOT be nested."
    )
    assert (
        actual_requirements.get(
            "../duvet-python/spec/spec.txt#2.2.1$A requirements section MUST " "be the top level containing header."
        ).content
        == "A requirements section MUST be the top level containing header."
    )
    assert (
        actual_requirements.get(
            "../duvet-python/spec/spec.txt#2.2.1$A header MUST NOT itself be a requirement."
        ).content
        == "A header MUST NOT itself be a requirement."
    )
    assert (
        actual_requirements.get(
            "../duvet-python/spec/spec.txt#2.2.1$A section MUST be indexable by combining different levels of naming."
        ).content
        == "A section MUST be indexable by combining different levels of naming."
    )