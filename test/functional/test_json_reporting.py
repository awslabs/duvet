# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Functional testing for config parsing"""

import pytest

from duvet.json_report import JSONReport
from duvet.spec_toml_parser import TomlRequirementParser

pytestmark = [pytest.mark.local, pytest.mark.functional]


def test_dogfood(pytestconfig):
    filepath = pytestconfig.rootpath.joinpath("duvet-specification")
    patterns = "compliance/**/*.toml"
    test_report = TomlRequirementParser().extract_toml_specs(patterns, filepath)

    actual_json = JSONReport()
    actual_json.from_report(test_report)
    actual_json.write_json()
    assert len(actual_json.specifications.keys()) == 1


def test_hello_world(pytestconfig):
    filepath = pytestconfig.rootpath.joinpath("examples/hello-world/hello-world-specification")
    patterns = "compliance/**/*.toml"
    test_report = TomlRequirementParser().extract_toml_specs(patterns, filepath)

    actual_json = JSONReport()
    actual_json.from_report(test_report)
    actual_json.write_json()
    assert len(actual_json.specifications.keys()) == 1
