# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Specification Parser used by duvet-python for toml format."""
import pytest

from duvet.spec_toml_parser import extract_toml_specs

pytestmark = [pytest.mark.unit, pytest.mark.local]


def test_extreact_toml_spec():
    path = "../"
    patterns = "compliance/**/*.toml"
    test_report = extract_toml_specs(patterns, path)
    # Verify one spec is added to the report object
    assert len(test_report.specifications.keys()) == 1
