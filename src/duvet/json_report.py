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
from pathlib import Path
from typing import List, Optional

import attr
from attrs import define, field

from duvet._config import Config
from duvet.formatter import clean_content
from duvet.identifiers import DEFAULT_JSON_PATH, AnnotationType
from duvet.refs_json import REFS_JSON
from duvet.specification_parser import Span
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
            result.update({"spec": self.spec})
        if self.citation:
            result.update({"citation": self.citation})
        if self.implication:
            result.update({"implication": self.implication})
        if self.test:
            result.update({"test": self.test})
        if self.exception:
            result.update({"exception": self.exception})
        if self.todo:
            result.update({"todo": self.todo})
        if self.level is not None:
            result.update({"level": self.level})

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
            result.update({"spec": len(requirement.content)})
        if self.citation:
            result.update({"citation": len(requirement.content)})
        if self.implication:
            result.update({"implication": len(requirement.content)})
        if self.test:
            result.update({"test": len(requirement.content)})
        if self.exception:
            result.update({"exception": len(requirement.content)})
        if self.todo:
            result.update({"todo": len(requirement.content)})

        if len(annotation_indexes) > 0:
            result.update({"related": annotation_indexes})

        return result


@define
class JSONReport:
    """Container of JSON report."""

    blob_link: str = field(init=True)
    issue_link: str = field(init=True)
    config_path = field(init=True)
    specifications: dict = field(init=False, default=attr.Factory(dict))
    annotations: list = field(init=False, default=attr.Factory(list))
    statuses: dict = field(init=False, default=attr.Factory(dict))
    refs: list[dict] = REFS_JSON

    @classmethod
    def create(cls, report: Report, config: Config):
        """Create a JSON Report."""
        rtn = JSONReport(blob_link=config.blob_url, issue_link=config.issue_url, config_path=config.config_path)
        for specification in report.specifications.values():
            rtn._process_specification(specification)
        return rtn

    @staticmethod
    def _process_lines(quotes, lines) -> list[list]:
        """Given a span of content, return a list of key word arguments of requirement."""
        requirements: list = []
        requirement_dict: dict = {}
        new_lines: list = []

        # Find requirement in the quotes.
        prev = 0
        index = 0
        while index < len(lines):
            line = lines[index]
            start = quotes.find(line[0][2])
            end = start + len(line[0][2])
            requirement = Span(start, end)
            requirements.append(requirement)
            requirement_dict[requirement.start] = index
            index += 1

        for requirement in requirements:
            if requirement.start <= prev:
                new_lines.append(lines[requirement_dict[requirement.start]])
            else:
                new_lines.append(clean_content(quotes[prev : requirement.start]))
                new_lines.append(lines[requirement_dict[requirement.start]])
            prev = requirement.end
        if prev < len(quotes) - 1:
            new_lines.append(clean_content(quotes[prev : len(quotes) - 1]))
        return lines

    def _process_section(self, section: Section) -> dict:
        # Half basked section dictionary.
        section_dict: dict = {
            "id": section.uri.split("#", 1)[1],  # This might break the front end, we will see.
            "title": section.title,
            "lines": section.lines,
        }

        # Get quotes from line.
        lines: list = []
        section_lines = [line[1:] for line in section.lines[1:]]
        quotes = "".join(section_lines)

        if len(section.lines) != 0:
            title_line = section.lines[0]
            title = title_line.rsplit(maxsplit=1)[1]
        else:
            # number, title = section_dict.get("id"), section_dict.get("title")
            title = section_dict.get("title")

        section_dict["title"] = title

        # Add specification index number if section has requirements.
        if section.has_requirements:
            requirement_index = []
            for requirement in section.requirements.values():
                requirement_index.append(self._process_requirement(requirement, section, lines))

            lines = self._process_lines(quotes, lines)

            section_dict.update({"requirements": requirement_index, "lines": lines})

        return section_dict

    def _process_sections(self, sections_dict: dict) -> List[List]:
        sections: List[dict] = []
        requirements: List[int] = []

        # Get sections and requirements dictionary list from section objects.
        for section in sections_dict.values():
            section_dict = self._process_section(section)
            sections.append(section_dict)
            requirements.extend(section_dict.get("requirements", []))

        # sections.extend([CONVENTIONS_AND_DEFINITIONS, NORMATIVE_REFERENCES])
        sections = sorted(sections, key=lambda d: d["id"])
        return [sections, requirements]

    def _process_specification(self, specification: Specification):
        """Serialize attributes from specification."""
        sections, requirements = self._process_sections(specification.sections)

        # Create specification dictionary
        # and add it to self.specifications.
        specification_dict: dict = {specification.source: {"requirements": requirements, "sections": sections}}
        self.specifications.update(specification_dict)

        return specification.source

    def _process_requirement(self, requirement: Requirement, section: Section, lines: list) -> int:
        """Serialize attributes from requirements.

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
            new_ref.from_annotation(annotation)
            self._process_annotation(annotation)
            annotation_indexes.append(len(self.annotations) - 1)

        line: list = []
        line_requirement: list = []
        annotation_indexes.append(len(self.annotations))
        line_requirement.append(annotation_indexes)

        # Add reference index to line.
        line_requirement.append(self.refs.index(new_ref.get_dict()))
        line_requirement.append(" ".join(requirement.content.split()))
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

    def _process_annotation(self, annotation: Annotation) -> int:
        """Serialize annotation dictionary from annotation object."""
        source = annotation.source
        target_path = annotation.target.split("#", 1)[0]
        target_section = annotation.target.split("#", 1)[1]
        line = annotation.start_line

        relative_source = Path(source).relative_to(self.config_path)

        result = {
            "source": str(relative_source),
            "target_path": target_path,
            "target_section": target_section,
            "line": line + 1,
            "type": annotation.type.name,
        }

        self.annotations.append(result)
        return len(self.annotations) - 1

    def get_dictionary(self) -> dict:
        """Return final JSON data."""
        result = {
            "blob_link": self.blob_link,
            "issue_link": self.issue_link,
            "specifications": self.specifications,
            "annotations": self.annotations,
            "statuses": self.statuses,
            "refs": self.refs,
        }
        return result

    def write_json(self, json_path: str = DEFAULT_JSON_PATH):
        """Write json file."""
        with open(json_path, "w+", encoding="utf-8") as json_result:
            json.dump(self.get_dictionary(), json_result)
