# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Test TOML spec"""
import pytest

from duvet.toml_requirement_parser import TomlRequirementParser
from duvet.toml_requirement_writer import TomlRequirementWriter

from .constants import TEST_SPEC_TOML_TARGET, TEST_SPEC_TOML_SPEC  # isort: skip

from ..utils import populate_file  # isort: skip

pytestmark = [pytest.mark.local, pytest.mark.functional]


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
