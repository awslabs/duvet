# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Requirement Parser used by duvet-python."""
import os
import pathlib
from typing import List, Optional

import toml
from attrs import define, field

from duvet.markdown import MarkdownSpecification
from duvet.requirement_parser import RequirementParser, create_requirements_from_list_to_section
from duvet.structures import Report, Section, Specification

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

    def to_toml(self, spec_dir):
        """Covert markdown section to toml files."""

        with open(self.title + ".toml", "w", encoding="utf-8") as temp_file:
            temp_heading = self.title.split(".")
            target = spec_dir + "#" + temp_heading[-1]
            for temp_req in self.requirements.values():
                temp_dict = {"level": temp_req.requirement_level.name, "quote": temp_req.content}
                self.spec_dict.append(temp_dict)

            toml_dict = {"target": target, "spec": self.spec_dict}
            toml.dump(toml_dict, temp_file)

    def _extract_requirements(self) -> bool:
        req_list_str = RequirementParser().extract_requirements(self.quotes)
        return create_requirements_from_list_to_section(self.section, req_list_str)


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

        parser = MarkdownSpecification.parse(markdown_spec_dir)
        spec = Specification(markdown_spec_dir.name, str(os.path.relpath(markdown_spec_dir, pathlib.Path.cwd())))
        return cls(parser, spec, markdown_spec_dir)

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

    def get_spec(self) -> Specification:
        """Add sections to existing spec or new spec."""

        for md_section in self._get_md_sections():
            self.spec.add_section(md_section.section)
        return self.spec
