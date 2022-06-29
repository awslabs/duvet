# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Integration test suite for duvet Annotation Parser."""


import pytest

from duvet.annotation_parser import AnnotationParser


@pytest.mark.integ
def test_extract_python_implementation_annotation(pytestconfig, caplog):
    actual_paths = list(pytestconfig.rootpath.glob("src/**/*.py"))
    actual_paths.extend(list(pytestconfig.rootpath.glob("test/**/*.py")))
    anno_meta_style = "# //="
    anno_content_style = "# //#"
    parser = AnnotationParser(actual_paths, anno_meta_style, anno_content_style)
    parser.process_all()


@pytest.mark.integ
def test_extract_dafny_implementation_annotation(pytestconfig, caplog):
    dfy_path = pytestconfig.rootpath
    actual_paths = list(dfy_path.glob("../aws-encryption-sdk-dafny/src/**/*.dfy"))
    actual_paths.extend(list(dfy_path.glob("../aws-encryption-sdk-dafny/test/**/*.dfy")))
    anno_meta_style = "# //="
    anno_content_style = "# //#"
    parser = AnnotationParser(actual_paths, anno_meta_style, anno_content_style)
    parser.process_all()
