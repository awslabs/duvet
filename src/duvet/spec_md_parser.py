# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Requirement Parser used by duvet-python."""
import os
import pathlib
from typing import List, Optional

from attrs import define, field

from duvet.markdown import MarkdownSpecification
from duvet.requirement_parser import RequirementParser
from duvet.structures import Report, Requirement, Section, Specification

__all__ = ["MDRequirementParser"]


@define
class MDRequirementParser:
    """Parser for specification in markdown format."""

    @staticmethod
    def extract_md_specs(patterns: str, path: pathlib.Path, md_report: Optional[Report] = None) -> Report:
        """Given pattern and filepath of markdown specs.

        Return or create a report.
        """
        if md_report is None:
            md_report = Report()
        for temp_md in pathlib.Path(path).glob(patterns):
            md_spec = MDSpec.load(temp_md)
            md_report.add_specification(md_spec)
        return md_report


@define
class MDSection:
    """Container of a markdown section."""

    title: str
    spec_dict: list = field(init=False, default=[])
    start_line: int
    end_line: int
    section: Section = field(init=False)
    quotes: str
    markdown_spec_dir: pathlib.Path

    def __attrs_post_init__(self):
        self.section = Section(
            self.title,
            "#".join([str(self.markdown_spec_dir.resolve()), self.title.split(".")[-1]]),
            self.start_line,
            self.end_line,
        )
        self._extract_requirements()

    def _extract_requirements(self) -> bool:
        req_kwargs: List[dict] = RequirementParser(self.quotes).extract_requirements([(0, len(self.quotes))])
        for kwarg in req_kwargs:
            content: Optional[str] = kwarg.get("content")
            if content is None:
                return False
            kwarg.setdefault("uri", "$".join([self.section.uri, content]))
            self.section.add_requirement(Requirement(**kwarg))
        return True


@define
class MDSpec:
    """Parser for specification in Markdown."""

    parser: MarkdownSpecification = field(init=True)
    spec: Optional[Specification]
    filepath: pathlib.Path = field(init=True)

    @classmethod
    def load(cls, markdown_spec_dir: pathlib.Path):  # pylint:disable=R0914
        """Given a filepath of a markdown spec.

        Return a specification or none.
        """

        parser: MarkdownSpecification = MarkdownSpecification.parse(markdown_spec_dir)
        spec = Specification(markdown_spec_dir.name, str(os.path.relpath(markdown_spec_dir, pathlib.Path.cwd())))
        filepath = markdown_spec_dir
        return cls(parser, spec, filepath)

    def _get_md_sections(self) -> List[MDSection]:
        md_section_list = []
        for descendant in self.parser.descendants:
            # descendant.body_span
            # print( descendant.body_span)
            start_line = self.parser.content[: descendant.body_span.start].count("\n")
            end_line = self.parser.content[: descendant.body_span.end].count("\n")
            quotes = descendant.get_body()
            md_section_list.append(MDSection(descendant.get_url(), start_line, end_line, quotes, self.filepath))
        return md_section_list

    def get_spec(self) -> Optional[Specification]:
        """Add sections to existing spec or new spec."""

        for md_section in self._get_md_sections():
            if self.spec is not None:
                self.spec.add_section(md_section.section)
        return self.spec

#//= compliance/duvet-specification.txt#2.2.1
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
