# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Integration test suite for duvet Annotation Parser."""
import pytest

from duvet.annotation_parser import AnnotationParser
from .integration_test_utils import get_path_to_esdk_dafny


pytestmark = [pytest.mark.integ]


class TestAnnotationParserAgainstDafnyESDK:

    def test_extract_python_implementation_annotation(self, pytestconfig):
        actual_paths = list(pytestconfig.rootpath.glob("src/**/*.py"))
        actual_paths.extend(list(pytestconfig.rootpath.glob("test/**/*.py")))
        anno_meta_style = "# //="
        anno_content_style = "# //#"
        parser = AnnotationParser(actual_paths, anno_meta_style, anno_content_style)
        parser.process_all()


class TestAnnotationParserAgainstDuvet:

    def test_extract_dafny_implementation_annotation(self, pytestconfig):
        dfy_path = get_path_to_esdk_dafny()
        actual_paths = list(dfy_path.glob("src/**/*.dfy"))
        actual_paths.extend(list(dfy_path.glob("test/**/*.dfy")))
        anno_meta_style = "# //="
        anno_content_style = "# //#"
        parser = AnnotationParser(actual_paths, anno_meta_style, anno_content_style)
        parser.process_all()
