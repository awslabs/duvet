# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Run the checks."""
import pathlib
import shutil

import click

from duvet._config import Config

__all__ = ("run",)

# from annotation_parser import AnnotationParser
from duvet.spec_toml_parser import TomlRequirementParser


def _center_pad(*, message: str, pad: str) -> str:
    columns, _lines = shutil.get_terminal_size()
    return message.center(columns, pad)


def _result_pad(*, message: str, result: str) -> str:
    columns, _lines = shutil.get_terminal_size()
    pad_char = "."
    pad_width = columns - len(message) - len(result) - 1
    return f"{message}{pad_char * pad_width} {result}"


def _check_pass(filename: str):
    click.secho(_result_pad(message=filename, result="PASS"), fg="green")


def _check_fail(filename: str):
    click.secho(_result_pad(message=filename, result="FAIL"), fg="red", err=True)


def run(*, config: Config) -> bool:
    """Run all specification checks."""
    # Extractions
    # Because we currently got only toml parser, let's give a try.
    path = pathlib.Path("./duvet-specification").resolve()
    patterns = "compliance/**/*.toml"
    test_report = TomlRequirementParser.extract_toml_specs(patterns, path)
    # Extract all annotations.
    all_annotations = []
    for _impl_config in config.implementation_configs:
        pass
        # all_annotations.extend(AnnotationParser(impl_config.impl_filenames, impl_config.meta_style,
        #                                        impl_config.meta_style).extract_implementation_file_annotations())
    for anno in all_annotations:
        test_report.add_annotation(anno)
    return test_report.pass_fail
