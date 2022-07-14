# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Integration testing for HTML writer."""
import logging

import pytest

from duvet.annotation_parser import AnnotationParser
from duvet.html import HTMLReport
from duvet.json_report import JSONReport
from duvet.spec_toml_parser import TomlRequirementParser

pytestmark = [pytest.mark.integ]


def test_against_duvet(pytestconfig, caplog, tmp_path):
    filepath = pytestconfig.rootpath.joinpath("duvet-specification")
    patterns = "compliance/**/*.toml"
    test_report = TomlRequirementParser().extract_toml_specs(patterns, filepath)

    # Parse annotations from implementation files.
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
    assert counter >= 0
    test_report.analyze_annotations()

    actual_json = JSONReport()
    html_report = HTMLReport()
    html_report.data = actual_json.from_report(test_report)
    html_path = html_report.write_html(f"{tmp_path}/duvet-report.html")
    assert html_path.endswith(".html")


def test_hello_world(pytestconfig, caplog, tmp_path):
    # Parse specifications from toml files.
    filepath = pytestconfig.rootpath.joinpath("examples/hello-world/hello-world-specification")
    caplog.set_level(logging.INFO)
    patterns = "compliance/**/*.toml"
    test_report = TomlRequirementParser().extract_toml_specs(patterns, filepath)

    # Parse annotations from implementation files.
    actual_paths = list(pytestconfig.rootpath.joinpath("examples/hello-world/").glob("src/**/*.py"))
    anno_meta_style = "# //="
    anno_content_style = "# //#"

    parser = AnnotationParser(actual_paths, anno_meta_style, anno_content_style)
    actual = parser.process_all()
    counter = 0
    for annotation in actual:
        if test_report.add_annotation(annotation):
            counter += 1
    assert counter >= 0
    test_report.analyze_annotations()

    actual_json = JSONReport()
    actual_json.from_report(test_report)
    html_report = HTMLReport()
    html_report.data = actual_json.from_report(test_report)
    html_path = html_report.write_html(f"{tmp_path}/duvet-report.html")
    assert html_path.endswith(".html")
