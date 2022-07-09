# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Run the checks."""
import click  # type : ignore[import]

from duvet._config import Config
from duvet.annotation_parser import AnnotationParser

# from duvet.html import HTMLReport
# from duvet.json_report import JSONReport
from duvet.spec_toml_parser import TomlRequirementParser
from duvet.summary import SummaryReport

__all__ = ("run",)


def run(*, config: Config) -> bool:
    """Run all specification checks."""
    # Extractions
    # Because we currently got only toml parser, let's give a try.
    toml_files = [toml_spec for toml_spec in config.specs if toml_spec.suffix == ".toml"]
    test_report = TomlRequirementParser().extract_toml_specs(toml_files)

    # Extract all annotations.
    all_annotations: list = []
    for impl_config in config.implementation_configs:
        annotation_parser: AnnotationParser = AnnotationParser(
            impl_config.impl_filenames, impl_config.meta_style, impl_config.content_style
        )
        # print(annotation_parser)
        all_annotations.extend(annotation_parser.process_all())
        # print(annotation_parser.process_all())
    # print(all_annotations)
    counter = 0
    for anno in all_annotations:
        if test_report.add_annotation(anno):
            counter += 1
    # assert counter > 0
    # print(counter)
    # print(config.implementation_configs)
    # print(all_annotations)
    # Analyze report
    summary = SummaryReport(test_report, config)
    summary.analyze_report()

    # Print summary to command line.
    for specification in test_report.specifications.values():
        for section in list(specification.sections.values()):
            click.echo(summary.report_section(summary.analyze_stats(section)))

    # # Covert report into JSON format
    # actual_json = JSONReport()
    # json_report = actual_json.from_report(test_report)
    # actual_json.write_json()

    # # Covert JSON report into HTML
    # html_report = HTMLReport()
    # html_report.data = json_report
    # # html_report.write_html()
    # click.echo(f"""Writing HTML report to {html_report.write_html()}""")

    return test_report.report_pass
