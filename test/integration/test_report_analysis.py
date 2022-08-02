# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Integration test suite for duvet Summary Report."""
import logging

import pytest

from duvet.annotation_parser import AnnotationParser
from duvet.summary import SummaryReport
from duvet.toml_requirement_parser import TomlRequirementParser

from .integration_test_utils import get_path_to_esdk_dafny  # isort: skip

pytestmark = [pytest.mark.integ]


class TestSummaryReportAgainstDuvet:
    def test_extract_python_implementation_annotation(self, pytestconfig, caplog):
        caplog.set_level(logging.INFO)
        filepath = pytestconfig.rootpath.joinpath("duvet-specification")
        patterns = "compliance/**/*.toml"
        test_report = TomlRequirementParser().extract_toml_specs(patterns, filepath)

        actual_paths = list(pytestconfig.rootpath.glob("src/**/*.py"))
        actual_paths.extend(list(pytestconfig.rootpath.glob("test/**/*.py")))
        anno_meta_style = "# //="
        anno_content_style = "# //#"
        parser = AnnotationParser(actual_paths, anno_meta_style, anno_content_style)
        actual = parser.process_all()
        counter = 0
        for annotation in actual:
            if test_report.add_annotation(annotation):
                counter += 1
        assert counter > 0
        for record in caplog.records:
            assert record.levelname != "ERROR"

        test_report.analyze_annotations()
        assert not test_report.report_pass


class TestSummaryReportAgainstESDKDafny:
    def test_extract_dafny_implementation_annotation(self, pytestconfig, caplog):
        caplog.set_level(logging.INFO)
        dfy_path = get_path_to_esdk_dafny()

        filepath = dfy_path.joinpath("aws-encryption-sdk-specification")
        patterns = "compliance/**/*.toml"
        test_report = TomlRequirementParser().extract_toml_specs(patterns, filepath)

        actual_paths = list(dfy_path.glob("src/**/*.dfy"))
        actual_paths.extend(list(dfy_path.glob("test/**/*.dfy")))
        parser = AnnotationParser(actual_paths)
        actual = parser.process_all()
        counter = 0
        for annotation in actual:
            if test_report.add_annotation(annotation):
                counter += 1
        assert counter > 0
        for record in caplog.records:
            assert record.levelname != "ERROR"
        assert not SummaryReport(test_report).analyze_report()
