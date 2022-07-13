# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Specification Parser used by duvet-python for toml format."""
import logging
from pathlib import Path
from typing import Any, Dict, List, MutableMapping, Optional

import toml
from attr import define

from duvet.formatter import clean_content
from duvet.identifiers import RequirementLevel
from duvet.structures import Report, Requirement, Section, Specification
from duvet.spec_toml_parser import TOML_URI_KEY, TOML_SPEC_KEY, TOML_REQ_LEVEL_KEY, TOML_REQ_CONTENT_KEY

_LOGGER = logging.getLogger(__name__)
__all__ = ["TomlRequirementWriter"]


@define
class TomlRequirementWriter:
    """TOML specifications writer."""

    def from_section(self, section: Section, parent_path: Path) -> list[Path]:
        """Write TOML from Section."""

        section_path: Path = parent_path.joinpath(section.title + ".toml")
        with open(section_path, "w+") as section_file:
            heading = section.uri.rsplit(".", 1)
            target = self.url + "#" + heading[len(heading) - 1]
            requirements: list[dict] = []
            for requirement in section.requirements.values():
                temp_dict = {TOML_REQ_LEVEL_KEY: requirement.requirement_level.name,
                             TOML_REQ_CONTENT_KEY: requirement.content}
                requirements.append(temp_dict)

            section_toml = {TOML_URI_KEY: target, TOML_SPEC_KEY: requirements}
            self.dump(section_toml, section_file)
        return [section_path]

    def from_specification(self, specification: Specification, parent_path: Path) -> list[Path]:
        """Write TOML from Specification."""

        specification_path: Path = parent_path.joinpath(specification.title)
        section_paths: list[Path] = []

        for section in specification.sections.values():
            section_paths.extend(self.from_section(section, specification_path))
        return section_paths

    def from_report(self, report: Report, directory: Path) -> list[Path]:
        """Write TOML from Report."""

        section_paths: list[Path] = []
        for specification in report.specifications.values():
            section_paths.extend(self.from_specification(specification, directory))

        return section_paths
