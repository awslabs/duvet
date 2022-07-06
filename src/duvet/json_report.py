# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Write json file for Duvet.

Assumptions:
1. This json is based on there is no change in the react file, which only support legacy mode.
2. We will map the excused_exception and un_excused map to exception, because new mode ia not supported.
3. We try to use lines to get the content which is not supported in our implementation.
"""
from typing import Optional

import attr
from attrs import define, field

from duvet.identifiers import RequirementLevel
from duvet.refs_json import REFS_JSON
from duvet.structures import Section, Requirement, Specification, Report, Annotation
import json


# # Opening JSON file
# f = open("./view/result.json")
#
# # returns JSON object as
# # a dictionary
# data = json.load(f)
#
# # Iterating through the json
# # list
# print(data)
#


class RefStatus:
    """Ref status class adopts from rust implementation."""
    spec: bool = False
    citation: bool = False
    implication: bool = False
    test: bool = False
    exception: bool = False
    todo: bool = False
    level: Optional[RequirementLevel] = None

    def get_dict(self) -> dict:
        result: dict = {}
        if self.spec: result.update({"spec": self.spec})
        if self.citation: result.update({"citation": self.citation})
        if self.implication: result.update({"implication": self.implication})
        if self.test: result.update({"test": self.test})
        if self.exception: result.update({"exception": self.exception})
        if self.todo: result.update({"todo": self.todo})
        if self.level is not None:
            result.update({"level": self.level.name})

        return result

    def from_annotation(self, annotation: Annotation):
        if annotation.type.name.lower() == "citation": self.citation = True
        if annotation.type.name == "implication": self.implication = True
        if annotation.type.name == "test": self.test = True
        if annotation.type.name == "exception": self.exception = True


@define
class JSONReport:
    blob_link: str = field(init=False)
    issue_link: str = field(init=False)
    specifications: dict = field(init=False, default=attr.Factory(dict))
    annotations: list = field(init=False, default=attr.Factory(list))
    statuses: dict = field(init=False, default=attr.Factory(dict))
    refs: list = field(init=False, default=attr.Factory(list))

    def __attrs_post_init__(self):
        self.refs = REFS_JSON.get("refs")

    def _from_section(self, section: Section) -> dict:
        # Half basked section dictionary.
        section_dict: dict = {
            "id": section.uri.split("#",1)[1],  # This might break the front end, we will see.
            "title": section.title,
            "lines": section.lines
        }

        # Add specification index number if section has requirements.
        if section.has_requirements:
            requirement_index = []
            for requirement in section.requirements.values():
                requirement_index.append(self.from_requirement(requirement, section))
            section_dict.update({"requirements": requirement_index})

        return section_dict

    def _from_sections(self, sections_dict: dict) -> (list[dict], list[int]):
        sections: list[dict] = []
        requirements: list[int] = []

        # Get sections for
        for section in sections_dict.values():
            sections.append(self._from_section(section))

            for requirement in section.requirements.values():
                requirements.append(self.from_requirement(requirement, section))

        # some operations
        return sections, requirements

    def from_specification(self, specification: Specification) -> bool:
        sections, requirements = self._from_sections(specification.sections)

        # some operations

        specification_dict: dict = {
            specification.source: {"requirements": requirements, "sections": sections}}
        self.specifications.update(specification_dict)
        # some operations

        return True

    def from_report(self, report: Report) -> bool:
        self.blob_link: str = "report.blob_link"
        self.issue_link: str = "report.issue_link"
        # specifications: dict = {}

        for specifications in report.specifications.values():
            self.from_specification(specifications)

        return self._get_dictionary()

    def from_requirement(self, requirement: Requirement, section: Section) -> int:
        source = requirement.uri.split("#", 1)[0]
        target_path = requirement.uri.split("#", 1)[0]
        target_section = section.title
        line = -1  # TODO: Figure out what it means.

        new_ref = RefStatus()
        for annotation in requirement.matched_annotations:
            new_ref.from_annotation(annotation)
            self.from_annotation(annotation)

        # self.refs.append(new_ref.get_dict())

        result = {
            "source": source,
            "target_path": target_path,
            "target_section": target_section,
            "type": "SPEC",
            "level:": requirement.requirement_level.name,
            "comment": requirement.content
        }

        # append result last because we want to know the index of this "annotation"
        self.annotations.append(result)

        return len(self.annotations) - 1

    def from_annotation(self, annotation: Annotation) -> int:
        source = annotation.source
        target_path = annotation.target.split("#", 1)[0]
        target_section = annotation.target.split("#", 1)[1]
        line = annotation.start_line  # TODO: Figure out what it means. line number in the section
        type = annotation.type.name

        result = {
            "source": source,
            "target_path": target_path,
            "target_section": target_section,
            "line": line,
            "type": type
        }

        self.annotations.append(result)
        return len(self.annotations) - 1

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
        with open("duvet-result.json", "w+", encoding="utf-8") as json_result:
            json.dump(self._get_dictionary(), json_result)
