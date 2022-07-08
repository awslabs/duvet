# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Write json file for Duvet.

Assumptions:
1. This json is based on there is no change in the React file, which only support legacy mode.
2. We will map the excused_exception and un_excused map to exception, because new mode is not supported by frontend.
3. We try to use lines to get the content which is not supported in our implementation.
4. We intentionally leave out the plain text of the sections, because it will introduce rfc parser which will
be in the next PR
"""
import json
from typing import List, Optional

import attr
from attrs import define, field

from duvet._config import Config
from duvet.identifiers import AnnotationType
from duvet.refs_json import REFS_JSON
from duvet.structures import Annotation, Report, Requirement, Section, Specification


class RefStatus:
    """Ref status class adopts from rust implementation."""

    spec: bool = False
    citation: bool = False
    implication: bool = False
    test: bool = False
    exception: bool = False
    todo: bool = False
    level: Optional[str] = None

    def get_dict(self) -> dict:
        """Get dictionary of refs from RefStatus Object."""
        result: dict = {}
        if self.spec:
            result["spec"] = self.spec
        if self.citation:
            result["citation"] = self.citation
        if self.implication:
            result["implication"] = self.implication
        if self.test:
            result["test"] = self.test
        if self.exception:
            result["exception"] = self.exception
        if self.todo:
            result["todo"] = self.todo
        if self.level is not None:
            result["level"] = self.level

        return result

    def from_annotation(self, annotation: Annotation):
        """Parse attributes from annotation."""
        if annotation.type == AnnotationType.CITATION:
            self.citation = True
        if annotation.type == AnnotationType.IMPLICATION:
            self.implication = True
        if annotation.type == AnnotationType.TEST:
            self.test = True
        if annotation.type == AnnotationType.EXCEPTION:
            self.exception = True
        if annotation.type == AnnotationType.TODO:
            self.todo = True

    def get_status(self, requirement: Requirement, annotation_indexes: list) -> dict:
        """Get dictionary of status from RefStatus Object."""
        result: dict = {}
        if self.spec:
            result["spec"] = len(requirement.content)
        if self.citation:
            result["citation"] = len(requirement.content)
        if self.implication:
            result["implication"] = len(requirement.content)
        if self.test:
            result["test"] = len(requirement.content)
        if self.exception:
            result["exception"] = len(requirement.content)
        if self.todo:
            result["todo"] = len(requirement.content)

        if len(annotation_indexes) > 0:
            result["related"] = annotation_indexes

        return result


@define
class JSONReport:
    """Container of JSON report."""

    blob_link: str = field(init=False, default="https://github.com/awslabs/duvet/blob/")
    issue_link: str = field(init=False, default="https://github.com/awslabs/duvet/issues")
    specifications: dict = field(init=False, default=attr.Factory(dict))
    annotations: list = field(init=False, default=attr.Factory(list))
    statuses: dict = field(init=False, default=attr.Factory(dict))
    refs: list[dict] = REFS_JSON

    def _from_section(self, section: Section) -> dict:
        # Half basked section dictionary.
        section_dict: dict = {
            "id": section.uri.split("#", 1)[1],  # This might break the front end, we will see.
            "title": section.title,
            "lines": section.lines,
        }

        lines: list = []

        # Add specification index number if section has requirements.
        if section.has_requirements:
            requirement_index = []
            for requirement in section.requirements.values():
                requirement_index.append(self.from_requirement(requirement, section, lines))
            section_dict.update({"requirements": requirement_index, "lines": lines})

        return section_dict

    def _from_sections(self, sections_dict: dict) -> List[List]:
        sections: List[dict] = []
        requirements: List[int] = []

        # Get sections and requirements dictionary list from section objects.
        for section in sections_dict.values():
            section_dict = self._from_section(section)
            sections.append(section_dict)
            requirements.extend(section_dict.get("requirements", []))

        return [sections, requirements]

    def from_specification(self, specification: Specification) -> str:
        """Parse attributes from specification."""
        sections, requirements = self._from_sections(specification.sections)

        # Create specification dictionary
        # and add it to self.specifications.
        specification_dict: dict = {specification.source: {"requirements": requirements, "sections": sections}}
        self.specifications.update(specification_dict)

        return specification.source

    def from_config(self, config: Config):
        """Parse attributes from Config."""
        self.blob_link: str = config.blob_url
        self.issue_link: str = config.issue_url

    def from_report(self, report: Report) -> dict:
        """Parse attributes from report."""
        for specifications in report.specifications.values():
            self.from_specification(specifications)

        return self._get_dictionary()

    def from_requirement(self, requirement: Requirement, section: Section, lines: list) -> int:
        """Parse attributes from requirements.

        Return index in the self.requirements.
        """
        source = requirement.uri.split("#", 1)[0]
        target_path = requirement.uri.split("#", 1)[0]
        target_section = section.title

        # Set up ref based on the requirement.
        new_ref = RefStatus()
        new_ref.spec = True
        new_ref.level = requirement.requirement_level.name

        # Get annotation indexes and reference
        annotation_indexes = []
        for annotation in requirement.matched_annotations:
            # print(annotation.type)
            new_ref.from_annotation(annotation)
            self.from_annotation(annotation)
            annotation_indexes.append(len(self.annotations) - 1)

        line: list = []
        line_requirement: list = []
        annotation_indexes.append(len(self.annotations))
        line_requirement.append(annotation_indexes)

        # Add reference index to line.
        line_requirement.append(self.refs.index(new_ref.get_dict()))
        line_requirement.append(requirement.content)
        line.append(line_requirement)
        lines.append(line)

        result = {
            "source": source,
            "target_path": target_path,
            "target_section": target_section,
            "type": "SPEC",
            "level": requirement.requirement_level.name,
            "comment": requirement.content,
        }

        # Append result last because we want to know the index of this "annotation".
        self.annotations.append(result)

        # Append statuses using the index of the requirement
        status = new_ref.get_status(requirement, annotation_indexes)
        self.statuses.update({str(len(self.annotations) - 1): status})

        return len(self.annotations) - 1

    def from_annotation(self, annotation: Annotation) -> int:
        """Parse annotation dictionary from annotation object."""
        source = annotation.source
        target_path = annotation.target.split("#", 1)[0]
        target_section = annotation.target.split("#", 1)[1]
        line = annotation.start_line

        result = {
            "source": source,
            "target_path": target_path,
            "target_section": target_section,
            "line": line,
            "type": annotation.type.name,
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
            "refs": self.refs,
        }
        return result

    def write_json(self, json_path: str = "duvet-result.json"):
        """Write json file."""
        with open(json_path, "w+", encoding="utf-8") as json_result:
            json.dump(self._get_dictionary(), json_result)
