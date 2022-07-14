# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Integration testing for HTML writer."""
import logging

import pytest

from duvet._config import Config
from duvet.annotation_parser import AnnotationParser
from duvet.html import HTMLReport
from duvet.json_report import JSONReport
from duvet.spec_toml_parser import TomlRequirementParser

from ..utils import populate_file  # isort: skip

pytestmark = [pytest.mark.integ]

SPEC_BLOCK = """[spec.markdown]
patterns = ["project-specification/**/*.md"]"""

IMPL_BLOCK = """[implementation]
[implementation.rs]
patterns = ["src/**/*.rs", "test/**/*.rs", "compliance_exceptions/**/*.txt"]
comment-style = { meta = "//=", content = "//#" }
[implementation.dfy]
patterns = ["src/**/*.dfy", "test/**/*.dfy", "compliance_exceptions/**/*.txt"]"""

REPORT_BLOCK = """[report]
[report.blob]
url = ["https://github.com/aws/aws-encryption-sdk-dafny/blob/"]
[report.issue]
url = ["https://github.com/aws/aws-encryption-sdk-dafny/issues"]"""


@pytest.fixture
def actual_config(tmp_path) -> Config:
    actual_path = populate_file(tmp_path, "\n".join([SPEC_BLOCK, IMPL_BLOCK, REPORT_BLOCK]), "duvet_config.toml")
    with pytest.warns(UserWarning):
        actual_config = Config.parse(str(actual_path.resolve()))
    return actual_config


def test_against_duvet(pytestconfig, caplog, tmp_path, actual_config):
    filepath = pytestconfig.rootpath.joinpath("duvet-specification")
    test_report = TomlRequirementParser().extract_toml_specs("compliance/**/*.toml", filepath)

    # Parse annotations from implementation files.
    actual_paths = list(pytestconfig.rootpath.glob("src/**/*.py"))
    actual_paths.extend(list(pytestconfig.rootpath.glob("test/**/*.py")))
    anno_meta_style = "# //="
    anno_content_style = "# //#"

    parser = AnnotationParser(actual_paths, anno_meta_style, anno_content_style)
    actual = parser.process_all()
    counter = [test_report.add_annotation(annotation) is True for annotation in actual].count(True)
    assert counter > 0
    test_report.analyze_annotations()

    actual_json = JSONReport.create(test_report, actual_config)
    html_report = HTMLReport()
    html_report.data = actual_json.get_dictionary()
    html_path = html_report.write_html(f"{tmp_path}/duvet-report.html")
    assert html_path.endswith(".html")


def test_hello_world(pytestconfig, caplog, tmp_path, actual_config):
    # Parse specifications from toml files.
    filepath = pytestconfig.rootpath.joinpath("examples/hello-world/hello-world-specification")
    caplog.set_level(logging.INFO)
    test_report = TomlRequirementParser().extract_toml_specs("compliance/**/*.toml", filepath)

    # Parse annotations from implementation files.
    actual_paths = list(pytestconfig.rootpath.joinpath("examples/hello-world/").glob("src/**/*.py"))
    anno_meta_style = "# //="
    anno_content_style = "# //#"

    parser = AnnotationParser(actual_paths, anno_meta_style, anno_content_style)
    actual = parser.process_all()
    counter = [test_report.add_annotation(annotation) is True for annotation in actual].count(True)
    assert counter > 0
    test_report.analyze_annotations()

    actual_json = JSONReport.create(test_report, actual_config)
    html_report = HTMLReport()
    html_report.data = actual_json.get_dictionary()
    html_path = html_report.write_html(f"{tmp_path}/duvet-report.html")
    assert html_path.endswith(".html")
