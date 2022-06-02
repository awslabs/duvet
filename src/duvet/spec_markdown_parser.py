# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Markdown Parser used by duvet-python."""
import queue
import re

import toml

from structures import *
from duvet.identifiers import RequirementLevel
from attrs import define, field

# from transitions import Machine may not in this

markdown_spec_dir = "../../spec/spec.md"
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


@define
class Specification:
    title: str
    url: str
    sections: dict  # hashmap equivalent in python


@define
class Section:
    # id: str
    # title: str
    full_title: str
    start_line: int
    end_line: int
    is_section: bool
    requirements: dict = field(init=False, default={})



@define
class Requirement:
    requirement_level: RequirementLevel
    implemented: bool
    attested: bool
    omitted: bool
    content: str
    id: str


curr_line = 0

spec = Specification("spec", markdown_spec_dir, {})
# print(spec)
section_lists = []
section_start = 0
section_end = 0

curr_section = Section(h4, curr_line, curr_line, False)
# for this toy script we only support update to h4
# which would be an equivalent to rfc in 1.1.1
while curr_line < len(lines):
    line = lines[curr_line]
    if "#### " in line:
        target = line.replace("#### ", h1 + ".").replace(" ", "-").removesuffix("\n").lower()
        h4 = line.replace("#### ", h3 + ".").replace(" ", "-").removesuffix("\n").lower()
        # set the end line of the previous section
        curr_section.end_line = curr_line - 1;
        # set the start line of the new section
        curr_section = Section(h4, curr_line, curr_line, False)
        # turn the new section to curr_section
        spec.sections[h4] = curr_section
        # print(target)
        # print(h4)
    elif "### " in line:
        target = line.replace("### ", h1 + ".").replace(" ", "-").removesuffix("\n").lower()
        h3 = line.replace("### ", h2 + ".").replace(" ", "-").removesuffix("\n").replace("/", "").lower()
        # set the end line of the previous section
        curr_section.end_line = curr_line - 1;
        # set the start line of the new section
        curr_section = Section(h3, curr_line, curr_line, False)
        # turn the new section to curr_section
        spec.sections[h3] = curr_section
        # print(target)
        # print(h3)
    elif "## " in line:
        h2 = line.removeprefix("## ").removesuffix("\n").lower()
        # set the end line of the previous section
        curr_section.end_line = curr_line - 1;
        # set the start line of the new section
        curr_section = Section(h2, curr_line, curr_line, False)
        # turn the new section to curr_section
        spec.sections[h2] = curr_section
        # print(h2)
    elif "# " in line and curr_line == 1:  # we do not really want the title here
        curr_state = "section_candidate"
        h1 = line.removeprefix("# ").replace(" ", "-").removesuffix("\n").lower()
        # print(h1)

    # increment current line by one
    curr_line += 1


# states = ["specification", "section"]
# @define
# class StateMachine:
#     def get_connection(self):


# Implemented = {"citation", "untestable", "deviation", "implication"}
# Attested = {"test", "untestable", "implication"}
# Ommitted = {"exception"}


def extract_requirements(section: Section, lines: list):
    section_curr_line = section.start_line
    # curr_requirement = Requirement(RequirementLevel.MUST,False,False,False,"","")
    reqs = {}
    while section_curr_line < section.end_line:
        print(section_curr_line)
        for section_line in lines[section_curr_line].removesuffix("\n").split(". "):
            # print(section_line)
            if "MUST MUST NOT" in section_line:
                # if '.' in section_line or '!' in section_line:
                curr_requirement = Requirement(RequirementLevel.MUST, False, False, False, section_line,
                                               section.full_title + "$" + section_line)
                print(curr_requirement)
                reqs[curr_requirement.id] = curr_requirement
                # pass
            # print(line)

            elif "MUST NOT" in section_line:
                # if '.' in section_line or '!' in section_line:
                curr_requirement = Requirement(RequirementLevel.MUST, False, False, False, section_line,
                                               section.full_title + "$" + section_line)
                # section.requirements[curr_requirement.id] = curr_requirement
                reqs[curr_requirement.id] = curr_requirement

            elif "MUST" in section_line:
                # print(section_line)
                # if '.' in section_line or '!' in section_line:
                curr_requirement = Requirement(RequirementLevel.MUST, False, False, False, section_line,
                                               section.full_title + "$" + section_line)
                reqs[curr_requirement.id] = curr_requirement
            elif "SHOULD SHOULD NOT" in section_line:
                # if '.' in section_line or '!' in section_line:
                temp = section.full_title + "$" + section_line
                curr_requirement = Requirement(RequirementLevel.SHOULD, False, False, False, section_line,temp)
                reqs[curr_requirement.id] = curr_requirement
                # print(curr_requirement)
        section_curr_line += 1
    section.requirements = reqs


for s in spec.sections.values():
    extract_requirements(s, lines)
    # print(s)

for s in spec.sections.values():
    with open(s.full_title + '.toml', 'w') as f:
        h = s.full_title.split(".")
        target = spec.url + "#" + h[len(h) - 1]
        specDict = []
        for r in s.requirements.values():
            temp_dict = {"level": r.requirement_level.name, "quote": r.content}
            specDict.append(temp_dict)

        st = {"target": target, "spec": specDict}
        new_toml_string = toml.dump(st, f)
# print(new_toml_string)


# print(spec)
