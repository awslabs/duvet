# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Integration test suite for duvet run checks."""
import pytest

from duvet._config import Config
from duvet._run_checks import DuvetController
from duvet.structures import Report

pytestmark = [pytest.mark.integ]


class TestRunChecksAgainstDuvet:
    def test_extract_python_implementation_annotation(self, pytestconfig):
        filepath = pytestconfig.rootpath.joinpath("duvet_config.toml")
        report = Report()
        config = Config.parse(filepath)
        actual_report = DuvetController.extract_toml(config, report)
        assert len(actual_report.specifications.keys()) == 1
