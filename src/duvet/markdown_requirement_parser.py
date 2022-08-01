# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Requirement Parser used by duvet-python."""
import copy
from pathlib import Path
from typing import List, Optional, Union

from attrs import define

from duvet.identifiers import ALL_MARKDOWN_LIST_ENTRY_REGEX
from duvet.markdown import MarkdownSpecification
from duvet.requirement_parser import RequirementParser
from duvet.structures import Report, Section, Specification


@define
class MarkdownRequirementParser(RequirementParser):
    """The parser of a requirement in a block."""

    @staticmethod
    def process_specifications(filepaths: list[Path], report: Optional[Report] = None) -> Report:
        """Given pattern and filepath of markdown specs.

        Return or create a report.
        """
        if report is None:
            report = Report()

        specifications: list[Specification] = []
        for filepath in filepaths:
            specifications.append(MarkdownRequirementParser._process_specification(filepath))

        for specification in specifications:
            report.add_specification(specification)

        return report

    @staticmethod
    def _process_specification(specification_source: Path) -> Specification:  # pylint:disable=R0914
        """Given a filepath of a markdown spec.

        Return a specification or none.
        """

        parser: Union[None, MarkdownSpecification] = None

        if specification_source.suffix == ".md":
            parser = MarkdownSpecification.parse(specification_source)

        specification = Specification(
            specification_source.name, str(specification_source.relative_to(specification_source.parent.parent))
        )

        for section in MarkdownRequirementParser._process_sections(parser, specification_source):
            if specification is not None:
                specification.add_section(section)

        return specification

    @staticmethod
    def _process_sections(parser, filepath) -> List[Section]:

        sections: list[Section] = []

        for descendant in parser.descendants:
            quotes: str = copy.deepcopy(descendant.get_body())

            lines = quotes.splitlines()
            lines[0] = "   ".join([descendant.get_number(), descendant.title])

            section_kwarg: dict = {
                "title": descendant.get_number().rstrip(". "),
                "start_line": parser.content[: descendant.body_span.start].count("\n"),
                "end_line": parser.content[: descendant.body_span.end].count("\n"),
                "lines": lines,
                "uri": "#".join(
                    [str(filepath.relative_to(filepath.parent.parent)), descendant.get_number().rstrip(". ")]
                ),
            }

            section = Section(**section_kwarg)

            section_with_requirements: list[Section] = [
                MarkdownRequirementParser._process_requirements(quotes, section, ALL_MARKDOWN_LIST_ENTRY_REGEX, False)
            ]

            sections.extend(section_with_requirements)

        return sections
