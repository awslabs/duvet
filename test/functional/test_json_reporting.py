# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Functional testing for config parsing"""
import logging

import pytest

from duvet._config import Config
from duvet.annotation_parser import AnnotationParser
from duvet.json_report import JSONReport
from duvet.spec_toml_parser import TomlRequirementParser

pytestmark = [pytest.mark.local, pytest.mark.functional]

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


def test_against_duvet(pytestconfig, tmp_path):
    actual_path = pytestconfig.rootpath.joinpath("duvet_config.toml")
    actual_config = Config.parse(str(actual_path.resolve()))

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
    assert counter > 0
    test_report.analyze_annotations()

    actual_json = JSONReport.create(test_report, actual_config)
    actual_json.write_json(tmp_path.joinpath("result.json"))
    assert len(actual_json.specifications.keys()) == 1


def test_hello_world(pytestconfig, caplog):
    actual_path = pytestconfig.rootpath.joinpath("examples/hello-world/duvet.toml")
    with pytest.warns(UserWarning):
        actual_config = Config.parse(str(actual_path.resolve()))

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
    annotations = parser.process_all()
    bool_list = [test_report.add_annotation(annotation) for annotation in annotations]
    assert bool_list.count(True) > 0
    test_report.analyze_annotations()

    actual_json = JSONReport.create(test_report, actual_config)
    actual_json.write_json()
    assert len(actual_json.specifications.keys()) == 1
    actual_specification = actual_json.specifications.get("compliance/hello-world.txt")
    assert len(actual_specification.get("sections")) == 3
    assert len(actual_specification.get("requirements")) == 2
    assert len(actual_json.annotations) == 3
