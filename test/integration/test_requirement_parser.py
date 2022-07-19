# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Integration test suite for duvet RFC Parser."""
import pytest

from duvet.annotation_parser import AnnotationParser
from duvet.requirement_parser import RequirementParser

from .integration_test_utils import get_path_to_esdk_dafny  # isort: skip

pytestmark = [pytest.mark.integ]


class TestRFCParserAgainstESDKDafny:
    def test_extract_dafny_implementation_annotation(self, pytestconfig):
        dfy_path = get_path_to_esdk_dafny()
        actual_paths = list(dfy_path.glob("aws-encryption-sdk-specification/compliance/**/*.txt"))

        report = RequirementParser.process_specifications(actual_paths)

        assert len(report.specifications.keys()) > 0
