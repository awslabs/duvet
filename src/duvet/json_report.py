# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Write json file for Duvet.

Assumptions:
1. This json is based on there is no change in the react file, which only support legacy mode.
2. We will map the excused_exception and un_excused map to exception, because new mode ia not supported.
3. We try to use lines to get the content which is not supported in our implementation.
"""
import pathlib

import attr
from attrs import define, field

from duvet.structures import Section, Requirement, Specification, Report
import json

# Opening JSON file
f = open("./view/result.json")

# returns JSON object as
# a dictionary
data = json.load(f)

# Iterating through the json
# list
print(data)


@define
class JSONReport:
    blob_link: str = field(init=False)
    issue_link: str = field(init=False)
    specifications: dict = field(init=False, default=attr.ib(dict))
    annotations: list = field(init=False, default=attr.ib(list))
    statuses: dict = field(init=False, default=attr.ib(dict))
    refs: list = field(init=False, default=attr.ib(list))

    def get_requirements(self, requirement: Requirement) -> list:
        requirements = []

        # some operations

        return requirements

    def _from_sections(self, sections_dict: dict) -> (list[dict], list[int]):
        sections: list[dict] = []
        requirements: list[int] = []

        # Get sections for
        for section in sections_dict.values():
            sections.append(self._from_section(section))
            section_dict: dict = {
                "id": section.uri,  # This might break the front end, we will see.
                "title": section.title,
                "lines": section.lines
            }
            sections.append(section_dict)

            for requirement in section.requirements.values():
                requirements.append(self.from_requirement(requirement))

        # some operations
        return (sections, requirements)

    def from_specification(self, specification: Specification) -> bool:
        requirements: list = []
        sections: list = []

        # some operations
        sections.append(self._from_sections(specification.section))

        specification_dict: dict = {specification.title: {requirements, sections}}
        self.specifications.update(specification_dict)
        # some operations

        return True

    def from_report(self, report: Report) -> bool:
        self.blob_link: str = report.blob_link
        self.issue_link: str = report.issue_link
        # specifications: dict = {}

        for specifications in report.specifications.values():
            self.from_specification(specifications)
        # annotations: list = []
        # statuses: dict = {}
        # refs: list = []

        return True

    def from_requirement(self, requirement: Requirement, section: Section) -> int:
        source = requirement.uri
        target_path = requirement.uri
        target_section = section.title
        line = -1  # TODO: Figure out what it means.
        type = requirement.requirement_level.name

        for annotation in requirement.matched_annotations:
            self.from_annotation(annotation)

        result = {
            "source": source,
            "target_path": target_path,
            "target_section": target_section,
            "line": line,
            "type": type
        }

        # append result last because we want to know the index of this "annotation"
        self.annotations.append(result)

        return len(self.annotations) - 1

    def from_annotation(self, Annotation) -> (dict, int):
        pass

    def _get_dictionary(self) -> dict:
        result = {
            "blob_link": self.blob_link,
            "issue_link": self.issue_link,
            "specifications": self.specifications,
            "annotations": self.annotations,
            "statuses": self.statuses,
            "refs": self.refs
        }
        return result

    def write_json(self):
        "write json file."
        with open("result.json", "w+", encoding="utf-8") as json_result:
            json.dump(self._get_dictionary, json_result)
