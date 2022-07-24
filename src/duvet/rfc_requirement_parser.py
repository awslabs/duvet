# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Requirement Parser used by duvet-python."""
import copy
from pathlib import Path
from typing import List, Optional

from attrs import define

from duvet.requirement_parser import RequirementParser
from duvet.rfc import RFCSpecification
from duvet.structures import Report, Section, Specification


@define
class RFCRequirementParser(RequirementParser):
    """The parser of a requirement in a block."""

    @staticmethod
    def process_specifications(
        filepaths: list[Path], spec_dir, report: Optional[Report] = None, is_legacy=False
    ) -> Report:
        """Given pattern and filepath of markdown specs.

        Return or create a report.
        """
        if report is None:
            report = Report()

        specifications: list[Specification] = []
        for filepath in filepaths:
            specifications.append(RFCRequirementParser._process_specification(filepath, spec_dir, is_legacy))

        for specification in specifications:
            report.add_specification(specification)
        return report

    @staticmethod
    def _process_specification(
        specification_source: Path, spec_dir, is_legacy=False
    ) -> Specification:  # pylint:disable=R0914
        """Given a filepath of a markdown spec.

        Return a specification or none.
        """

        parser: RFCSpecification = RFCSpecification.parse(specification_source)
        from_spec = specification_source.relative_to(spec_dir)
        specification = Specification(specification_source.name, str(Path(*from_spec.parts[1:])))

        for section in RFCRequirementParser._process_sections(parser, specification.source, is_legacy):
            if specification is not None:
                specification.add_section(section)

        return specification

    @staticmethod
    def _process_sections(parser, filepath, is_legacy) -> List[Section]:
        sections: list[Section] = []

        for descendant in parser.descendants:
            start_line = parser.content[: descendant.body_span.start].count("\n")
            end_line = parser.content[: descendant.body_span.end].count("\n")
            quotes: str = copy.deepcopy(descendant.get_body())

            lines = quotes.splitlines()
            lines[0] = "   ".join([descendant.number, descendant.title])

            section_kwarg: dict = {
                "title": descendant.number.rstrip(". "),
                "start_line": start_line,
                "end_line": end_line,
                "lines": lines,
                "uri": "#".join([str(filepath), descendant.number.rstrip(". ")]),
            }

            section = Section(**section_kwarg)

            section_with_requirements: list[Section] = [
                RFCRequirementParser._process_requirements(quotes, section, "RFC", is_legacy)]

            sections.extend(section_with_requirements)

        return sections

