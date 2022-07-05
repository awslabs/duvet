# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Functional testing for config parsing"""

import pytest

from duvet._config import Config, ImplConfig
from duvet.json_report import JSONReport
from duvet.spec_toml_parser import TomlRequirementParser

from ..utils import populate_file  # isort: skip

pytestmark = [pytest.mark.local, pytest.mark.functional]


def test_dogfood(pytestconfig):
    filepath = pytestconfig.rootpath.joinpath("duvet-specification")
    patterns = "compliance/**/*.toml"
    test_report = TomlRequirementParser().extract_toml_specs(patterns, filepath)

    actual_json = JSONReport()
    actual_json.from_report(test_report)
    actual_json.write_json()
    assert actual_json == []
