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
    def process_specifications(filepaths: list[Path], specification_path: Path, report: Optional[Report] = None,
                               is_legacy=False) -> Report:
        """Given pattern and filepath of markdown specs.

        Return or create a report.
        """
        if report is None:
            report = Report()

        specifications: list[Specification] = []
        for filepath in filepaths:
            specifications.append(RFCRequirementParser._process_specification(filepath, specification_path, is_legacy))

        for specification in specifications:
            report.add_specification(specification)

        print(report)

        return report

    @staticmethod
    def _process_specification(specification_source: Path, specification_path: Path,
                               is_legacy=False) -> Specification:  # pylint:disable=R0914
        """Given a filepath of a markdown spec.

        Return a specification or none.
        """

        parser: RFCSpecification = RFCSpecification.parse(specification_source)

        # print(specification_source.relative_to(
        #     specification_path.joinpath("aws-encryption-sdk-specification")))
        specification = Specification(
            specification_source.name, str(specification_source.relative_to(
                specification_path))
        )

        for section in RFCRequirementParser._process_sections(parser, specification_source, is_legacy):
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
                "uri": "#".join([str(filepath.relative_to(filepath.parent.parent)), descendant.number.rstrip(". ")]),
            }

            section = Section(**section_kwarg)

            section_with_requirements: list[Section] = []
            if filepath.suffix == ".txt":
                section_with_requirements.append(
                    RFCRequirementParser._process_requirements(quotes, section, "RFC", is_legacy)
                )

            sections.extend(section_with_requirements)

        return sections

# //= compliance/duvet-specification.txt#2.2.2
# //= type=implication
# //# A requirement MUST be terminated by one of the following:

# //= compliance/duvet-specification.txt#2.2.2
# //= type=implication
# //# In the case of requirement terminated by a list, the text proceeding the list MUST be concatenated with each
# //# element of the list to form a requirement.

# //= compliance/duvet-specification.txt#2.2.2
# //= type=implication
# //# List elements MAY have RFC 2119 keywords, this is the same as regular sentences with multiple keywords.


# //= compliance/duvet-specification.txt#2.3.6
# //= type=implication
# //# A one or more line meta part MUST be followed by at least a one line content part.

# //= compliance/duvet-specification.txt#2.2.2
# //= type=TODO
# //# Sublists MUST be treated as if the parent item were terminated by the sublist.


# //= compliance/duvet-specification.txt#2.2.1
# //# The name of the sections MUST NOT be nested.

# //= compliance/duvet-specification.txt#2.2.1
# //= type=exception
# //# A requirements section MUST be the top level containing header.

# //= compliance/duvet-specification.txt#2.2.1
# //= type=implication
# //# A header MUST NOT itself be a requirement.

# //= compliance/duvet-specification.txt#2.2.1
# //= type=TODO
# //# A section MUST be indexable by combining different levels of naming.

# //= compliance/duvet-specification.txt#2.2.2
# //= type=TODO
# //# Sublists MUST be treated as if the parent item were
# //# terminated by the sublist.
