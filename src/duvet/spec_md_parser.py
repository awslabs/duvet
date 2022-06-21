# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Requirement Parser used by duvet-python."""
import os
import pathlib
from typing import List, Optional

import toml
from attrs import define

from duvet.requirement_parser import (
    RequirementParser,
    create_requirements_from_list_to_section,
)
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
class MDSpec:
    """Parser for specification in Markdown."""

    @staticmethod
    def load(markdown_spec_dir: pathlib.Path) -> Optional[Specification]:  # pylint:disable=R0914
        """Given a filepath of a markdown spec.

        Return a specification or none.
        """
        with open(markdown_spec_dir, "r", encoding="utf-8") as markdown_spec:
            lines = markdown_spec.readlines()
        heading1 = "h1"
        heading2 = "h2"
        heading3 = "h3"
        heading4 = "h4"
        curr_line = 0
        spec = Specification(markdown_spec_dir.name, str(os.path.relpath(markdown_spec_dir, pathlib.Path.cwd())))
        curr_section = Section(heading4, "#".join([spec.spec_dir, heading4.rsplit(".", maxsplit=1)[-1]]), curr_line,
                               curr_line)
        # for this toy script we only support update to heading4
        # which would be an equivalent to rfc in 1.1.1
        while curr_line < len(lines):
            line = lines[curr_line]
            if "#### " in line:
                target = line.replace("#### ", heading1 + ".").replace(" ", "-").removesuffix("\n").lower()
                heading4 = line.replace("#### ", heading3 + ".").replace(" ", "-").removesuffix("\n").lower()
                # set the end line of the previous section
                curr_section.end_line = curr_line - 1
                # set the start line of the new section
                curr_section = Section(heading4, "#".join([spec.spec_dir, heading4.split(".")[-1]]), curr_line,
                                       curr_line)
                # turn the new section to curr_section
                spec.sections.setdefault(heading4, curr_section)
                # print(target)
                # print(heading4)
            elif "### " in line:
                target = line.replace("### ", heading1 + ".").replace(" ", "-").removesuffix("\n").lower()
                heading3 = line.replace("### ", heading2 + ".").replace(" ", "-").removesuffix("\n").replace("/",
                                                                                                             "").lower()
                # set the end line of the previous section
                curr_section.end_line = curr_line - 1
                # set the start line of the new section
                curr_section = Section(heading3, "#".join([spec.spec_dir, heading3.split(".")[-1]]), curr_line,
                                       curr_line)
                # turn the new section to curr_section
                spec.sections.setdefault(heading3, curr_section)
                # print(target)
                # print(heading3)
            elif "## " in line:
                heading2 = line.removeprefix("## ").removesuffix("\n").lower()
                # set the end line of the previous section
                curr_section.end_line = curr_line - 1
                # set the start line of the new section
                curr_section = Section(
                    heading2, "#".join([str(markdown_spec_dir.resolve()), heading2.split(".")[-1]]), curr_line,
                    curr_line
                )
                # turn the new section to curr_section
                spec.sections.setdefault(heading2, curr_section)
                # print(heading2)
            elif "# " in line and curr_line == 1:  # we do not really want the title here
                # curr_state = "section_candidate"
                heading1 = line.removeprefix("# ").replace(" ", "-").removesuffix("\n").lower()
                # print(heading1)

            # increment current line by one
            curr_line += 1

        # def extract_requirements(section: Section, lines: list):
        #     section_curr_line = section.start_line
        #     # curr_requirement = Requirement(RequirementLevel.MUST,False,False,False,"","")
        #     reqs = {}
        #     while section_curr_line < section.end_line:
        #         print(section_curr_line)
        #         for section_line in lines[section_curr_line].removesuffix("\n").split(". "):
        #             print(section_line)
        #         section_curr_line += 1
        #     section.requirements = reqs
        def _extract_requirements(section: Section, lines: List[str]) -> bool:
            quotes = "".join(lines[section.start_line: section.end_line])
            req_list_str = RequirementParser().extract_requirements(quotes)
            return create_requirements_from_list_to_section(section, req_list_str)

        for temp_section in spec.sections.values():
            _extract_requirements(temp_section, lines)
            # print(s)

        for temp_section in spec.sections.values():
            with open(temp_section.title + ".toml", "w", encoding="utf-8") as temp_file:
                temp_heading = temp_section.title.split(".")
                target = spec.spec_dir + "#" + temp_heading[-1]
                spec_dict = []
                for temp_req in temp_section.requirements.values():
                    temp_dict = {"level": temp_req.requirement_level.name, "quote": temp_req.content}
                    spec_dict.append(temp_dict)

                toml_dict = {"target": target, "spec": spec_dict}
                toml.dump(toml_dict, temp_file)
