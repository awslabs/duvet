# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Run the checks."""
from typing import Optional

import click  # type : ignore[import]
from attrs import define

from duvet._config import Config
from duvet.annotation_parser import AnnotationParser
from duvet.spec_toml_parser import TomlRequirementParser
from duvet.structures import Report
from duvet.summary import SummaryReport


def run(*, config: Config) -> bool:
    """Run all specification checks."""
    # Extractions
    # Because we currently got only toml parser, let's give a try.
    report = DuvetController.extract_toml(config, None)

    # Extract all annotations.
    DuvetController.extract_implementation(config, report)

    # Analyze report
    # Print summary to command line.
    DuvetController.write_summary(config, report)

    return report.report_pass


@define
class DuvetController:
    """Controller of Duvet's behavior"""

    @staticmethod
    def extract_toml(config: Config, report: Optional[Report] = None) -> Report:
        """Extract TOML files."""
        if report is None:
            report = Report()

        toml_files = [toml_spec for toml_spec in config.specs if toml_spec.suffix == ".toml"]
        report = TomlRequirementParser.extract_toml_specs(toml_files)

        return report

    @staticmethod
    def extract_implementation(config: Config, report: Optional[Report] = None) -> Report:
        """Extract all annotations in implementations."""
        if report is None:
            report = Report()

        all_annotations: list = []
        for impl_config in config.implementation_configs:
            annotation_parser: AnnotationParser = AnnotationParser(
                impl_config.impl_filenames, impl_config.meta_style, impl_config.content_style
            )
            all_annotations.extend(annotation_parser.process_all())

        all_annotations_added: list[bool] = [report.add_annotation(anno) for anno in all_annotations]
        click.echo(f"{all_annotations_added.count(True)} of {len(all_annotations_added)} added to the report")

        return report

    @staticmethod
    def write_summary(config: Config, report: Report):

        summary = SummaryReport(report, config)
        summary.analyze_report()

        # Print summary to command line.
        for specification in report.specifications.values():
            for section in list(specification.sections.values()):
                click.echo(summary.report_section(summary.analyze_stats(section)))
