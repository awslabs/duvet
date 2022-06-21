# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Requirement Parser used by duvet-python."""
import os
import pathlib
import re
import warnings
from typing import List, Optional

import attr
import toml
from attrs import define, field

from duvet.identifiers import RequirementLevel
from duvet.requirement_parser import RequirementParser, ALL_MARKDOWN_LIST_ENTRY_REGEX, SENTENCE_DIVIDER, \
    REQUIREMENT_IDENTIFIER_REGEX, _preprocess_inline_requirements, ListRequirements, _extract_inline_requirements, \
    create_requirements_from_list_to_section
from duvet.structures import Requirement, Section, Report, Specification

__all__ = ["MDRequirementParser"]


@define
class MDRequirementParser:

    @staticmethod
    def extract_md_specs(patterns: str, path: pathlib.Path, md_report: Optional[Report] = None) -> Report:
        if md_report is None:
            md_report = Report()
        for temp_md in pathlib.Path(path).glob(patterns):
            md_spec = MDSpec.load(temp_md)
            md_report.add_specification(md_spec)
        return md_report


@define
class MDSpec:

    @staticmethod
    def load(markdown_spec_dir: pathlib.Path) -> Optional[Specification]:
        markdown_spec = open(markdown_spec_dir, "r")
        lines = markdown_spec.readlines()
        h1 = "h1"
        h2 = "h2"
        h3 = "h3"
        h4 = "h4"
        h5 = "h5"
        section_state = ""
        requirement_state = ""
        specification_state = ""
        curr_line = 0
        spec = Specification(markdown_spec_dir.name, str(os.path.relpath(markdown_spec_dir, pathlib.Path.cwd())))
        # print(spec)
        section_lists = []
        section_start = 0
        section_end = 0
        curr_section = Section(h4, "#".join([spec.spec_dir, h4.split(".")[-1]]), curr_line, curr_line)
        # for this toy script we only support update to h4
        # which would be an equivalent to rfc in 1.1.1
        while curr_line < len(lines):
            line = lines[curr_line]
            if "#### " in line:
                target = line.replace("#### ", h1 + ".").replace(" ", "-").removesuffix("\n").lower()
                h4 = line.replace("#### ", h3 + ".").replace(" ", "-").removesuffix("\n").lower()
                # set the end line of the previous section
                curr_section.end_line = curr_line - 1
                # set the start line of the new section
                curr_section = Section(h4, "#".join([spec.spec_dir, h4.split(".")[-1]]), curr_line, curr_line)
                # turn the new section to curr_section
                spec.sections[h4] = curr_section
                # print(target)
                # print(h4)
            elif "### " in line:
                target = line.replace("### ", h1 + ".").replace(" ", "-").removesuffix("\n").lower()
                h3 = line.replace("### ", h2 + ".").replace(" ", "-").removesuffix("\n").replace("/", "").lower()
                # set the end line of the previous section
                curr_section.end_line = curr_line - 1
                # set the start line of the new section
                curr_section = Section(h3, "#".join([spec.spec_dir, h3.split(".")[-1]]), curr_line, curr_line)
                # turn the new section to curr_section
                spec.sections[h3] = curr_section
                # print(target)
                # print(h3)
            elif "## " in line:
                h2 = line.removeprefix("## ").removesuffix("\n").lower()
                # set the end line of the previous section
                curr_section.end_line = curr_line - 1
                # set the start line of the new section
                curr_section = Section(h2, "#".join([str(markdown_spec_dir.resolve()), h2.split(".")[-1]]), curr_line,
                                       curr_line)
                # turn the new section to curr_section
                spec.sections[h2] = curr_section
                # print(h2)
            elif "# " in line and curr_line == 1:  # we do not really want the title here
                curr_state = "section_candidate"
                h1 = line.removeprefix("# ").replace(" ", "-").removesuffix("\n").lower()
                # print(h1)

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
        def _extract_requirements(section: Section, lines: list) -> bool:
            quotes = "".join(lines[section.start_line:section.end_line])
            req_list_str = RequirementParser().extract_requirements(quotes)
            return create_requirements_from_list_to_section(section, req_list_str)

        for s in spec.sections.values():
            _extract_requirements(s, lines)
            print(s)

        for s in spec.sections.values():
            with open(s.title + ".toml", "w") as f:
                h = s.title.split(".")
                target = spec.spec_dir + "#" + h[- 1]
                spec_dict = []
                for r in s.requirements.values():
                    temp_dict = {"level": r.requirement_level.name, "quote": r.content}
                    spec_dict.append(temp_dict)

                st = {"target": target, "spec": spec_dict}
                new_toml_string = toml.dump(st, f)
