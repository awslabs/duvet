# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Run the checks."""
from typing import Optional

import click  # type : ignore[import]
from attr import define

from duvet._config import Config
from duvet.annotation_parser import AnnotationParser
from duvet.html import HTMLReport
from duvet.json_report import JSONReport
from duvet.markdown_requirement_parser import MarkdownRequirementParser
from duvet.rfc_requirement_parser import RFCRequirementParser
from duvet.structures import Report
from duvet.summary import SummaryReport
from duvet.toml_requirement_parser import TomlRequirementParser


def run(*, config: Config) -> bool:
    """Run all specification checks."""
    # Extractions

    report = Report()

    report = DuvetController.extract_rfc(config, report)

    report = DuvetController.extract_markdown(config, report)

    report = DuvetController.extract_toml(config, report)

    # DuvetController.extract_markdown(config, report)

    # Because we currently got only toml parser, let's give a try.
    # toml_files = [toml_spec for toml_spec in config.specs if toml_spec.suffix == ".toml"]
    # report = TomlRequirementParser().extract_toml_specs(toml_files)

    # Extract all annotations.
    DuvetController.extract_implementation(config, report)

    # Analyze report
    DuvetController.write_summary(config, report)

    DuvetController.write_html(config, report)

    return report.report_pass


@define
class DuvetController:
    """Controller of Duvet's behavior"""

    @staticmethod
    def extract_rfc(config: Config, report: Report) -> Report:
        """Extract rfc files."""
        rfc_files = [rfc_spec for rfc_spec in config.specs if rfc_spec.suffix == ".txt"]
        report = RFCRequirementParser.process_specifications(rfc_files, report, is_legacy=config.legacy)

        return report

    @staticmethod
    def extract_markdown(config: Config, report: Optional[Report] = None) -> Report:
        """Extract markdown files."""
        markdown_files: list = [markdown_spec for markdown_spec in config.specs if markdown_spec.suffix == ".md"]
        test_report = MarkdownRequirementParser.process_specifications(markdown_files, report)
        click.echo(test_report)
        return test_report

    @staticmethod
    def extract_toml(config: Config, report: Report) -> Report:
        """Extract TOML files."""
        print(config.specification_path)
        print("---------------------------")
        toml_files = [toml_spec for toml_spec in config.specs if toml_spec.suffix == ".toml"]
        report = TomlRequirementParser.extract_toml_specs(toml_files, report)

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
    def write_html(config: Config, report: Report) -> Report:

        # Covert report into JSON format
        actual_json = JSONReport.create(report, config)

        # Covert JSON report into HTML
        html_report = HTMLReport.from_json_report(actual_json)

        click.echo(f"""Writing HTML report to {html_report.write_html()}""")

        return report

    @staticmethod
    def write_summary(config: Config, report: Report):

        summary = SummaryReport(report, config)
        summary.analyze_report()

        # Print summary to command line.
        for specification in report.specifications.values():
            for section in list(specification.sections.values()):
                click.echo(summary.report_section(summary.analyze_stats(section)))
