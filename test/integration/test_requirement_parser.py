# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Integration test suite for duvet RFC Parser."""
import pytest

from duvet.requirement_parser import RequirementParser

from .integration_test_utils import get_path_to_esdk_dafny  # isort: skip

pytestmark = [pytest.mark.integ]


class TestRFCParserAgainstESDKDafny:
    def test_extract_dafny_implementation_annotation(self, pytestconfig):
        dfy_path = get_path_to_esdk_dafny()

        filepath = dfy_path.joinpath("aws-encryption-sdk-specification")

        actual_paths = list(filepath.glob("compliance/**/*.txt"))

        report = RequirementParser.process_specifications(actual_paths)

        assert len(report.specifications.keys()) > 0
