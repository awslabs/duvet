# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Integration test suite for duvet Summary Report."""
import logging

import pytest
from typing import Iterable

from pathlib import Path

from duvet.annotation_parser import AnnotationParser
from duvet.identifiers import AnnotationType
from duvet.spec_toml_parser import TomlRequirementParser
from duvet.structures import Annotation
from duvet.summary import SummaryReport

from .integration_test_utils import get_path_to_esdk_dafny  # isort: skip

pytestmark = [pytest.mark.integ]

# class TestSummaryReportAgainstDuvet:
#     def test_extract_python_implementation_annotation(self, pytestconfig):
#         filepath = pytestconfig.rootpath.joinpath("duvet-specification")
#         patterns = "compliance/**/*.toml"
#         test_report = TomlRequirementParser.extract_toml_specs(patterns, filepath)
#
#         actual_paths = list(pytestconfig.rootpath.glob("src/**/*.py"))
#         actual_paths.extend(list(pytestconfig.rootpath.glob("test/**/*.py")))
#         anno_meta_style = "# //="
#         anno_content_style = "# //#"
#         parser = AnnotationParser(actual_paths, anno_meta_style, anno_content_style)
#         actual = parser.process_all()
#
#         for annotation in actual:
#             test_report.add_annotation(annotation)
#
#         assert not test_report.report_pass


# class TestSummaryReportAgainstESDKDafny:
#     def test_extract_dafny_implementation_annotation(self, pytestconfig):
#         dfy_path = get_path_to_esdk_dafny()
#
#         filepath = dfy_path.joinpath("aws-encryption-sdk-specification")
#         patterns = "compliance/**/*.toml"
#         test_report = TomlRequirementParser.extract_toml_specs(patterns, filepath)
#
#         actual_paths = list(dfy_path.glob("src/**/*.dfy"))
#         actual_paths.extend(list(dfy_path.glob("test/**/*.dfy")))
#         parser = AnnotationParser(actual_paths)
#         actual = parser.process_all()
#
#         for annotation in actual:
#             test_report.add_annotation(annotation)
#
#         assert not SummaryReport(test_report).analyze_report()

_LOGGER = logging.getLogger('duvet_parse_anno')

logging.basicConfig(level=logging.INFO)


class TestSummaryReportAgainstESDKDafny:
    def test_esdk(self):
        annotation = Annotation(target='compliance/client-apis/client.txt#2.4.2',
                                type=AnnotationType.IMPLICATION,
                                content='Callers MUST have a way to disable this limit.',
                                start_line=99, end_line=103,
                                uri='compliance/client-apis/client.txt#2.4.2$Callers MUST have a way to disable this limit.',
                                location='/Users/yuancc/workspaces/aws-encryption-sdk-dafny/src/SDK/AwsEncryptionSdk.dfy',
                                reason=None)

        logging.basicConfig(level=logging.INFO)

        dfy_path = get_path_to_esdk_dafny()

        filepath = dfy_path.joinpath("aws-encryption-sdk-specification")
        patterns = "compliance/**/client/2.4.2.toml"
        test_report = TomlRequirementParser.extract_toml_specs(patterns, filepath)

        # test_paths: Iterable[Path] = dfy_path.glob("test/**/*.dfy")
        src_paths: Iterable[Path] = dfy_path.glob("src/**/AwsEncryptionSdk.dfy")
        for paths in [src_paths]:
            parser = AnnotationParser(paths=paths)
            for filepath in parser.paths:
                try:
                    annotations: list[Annotation] = parser.process_file(filepath)
                    print(annotations)
                    adds = [test_report.add_annotation(mem) for mem in annotations]
                    assert all(adds), f"Failed to add {len(adds) - sum(adds)} out of {len(adds)} to the report"
                except KeyboardInterrupt:
                    break
                except Exception as ex:
                    _LOGGER.error("%s: hit %s.", (str(filepath), ex), ex)
        summary = SummaryReport(test_report)
        assert annotation in test_report.specifications.get('compliance/client-apis/client.txt').sections.get(
            'compliance/client-apis/client.txt#2.4.2').requirements.get(
            'compliance/client-apis/client.txt#2.4.2$Callers MUST have a way to disable this limit.').matched_annotations
        rtn = summary.analyze_report()
        # noinspection PySimplifyBooleanCheck
        if rtn != False:
            _LOGGER.error(f"Summary.analyze should have been False, was {rtn}")
        return summary
