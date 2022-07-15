# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Test TOML spec"""
import pytest

from duvet.spec_toml_parser import TomlRequirementParser
from duvet.spec_toml_writer import TomlRequirementWriter

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


def test_dogfood(pytestconfig, tmp_path):
    filepath = pytestconfig.rootpath.joinpath("duvet-specification")
    patterns = "compliance/**/*.toml"
    test_report = TomlRequirementParser.extract_toml_specs(patterns, filepath)
    # Verify one spec is added to the report object
    TomlRequirementWriter.process_report(test_report, tmp_path.joinpath("compliance"))

    new_report = TomlRequirementParser.extract_toml_specs(patterns, tmp_path)
    assert test_report == new_report


def test_extract_spec_toml(tmp_path):
    # We will not throw error is there is no requirements.
    patterns = "compliance/**/*.toml"
    populate_file(tmp_path, "\n".join([TEST_SPEC_TOML_TARGET, TEST_SPEC_TOML_SPEC]), "compliance/spec/section1.toml")
    expected_report = TomlRequirementParser().extract_toml_specs(patterns, tmp_path)
    # Verify requirements is added to the report object

    section = expected_report.specifications.get("../duvet-python/spec/spec.txt").sections.get(
        "../duvet-python/spec/spec.txt#2.2.1"
    )

    TomlRequirementWriter._process_section(section, tmp_path)

    actual_report = TomlRequirementParser().extract_toml_specs(patterns, tmp_path)
    assert actual_report == expected_report
