# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Specification TOML writer used by duvet-python for toml format.

Assumptions:
1. TODO: Need to write lines when the HTML PR is merged.
"""
import logging
from pathlib import Path

import toml
from attr import define

from duvet.spec_toml_parser import TOML_REQ_CONTENT_KEY, TOML_REQ_LEVEL_KEY, TOML_SPEC_KEY, TOML_URI_KEY
from duvet.structures import Report, Section, Specification

_LOGGER = logging.getLogger(__name__)
__all__ = ["TomlRequirementWriter"]


@define
class TomlRequirementWriter:
    """TOML specifications writer."""

    @staticmethod
    def _process_section(section: Section, parent_path: Path) -> list[Path]:
        """Write TOML from Section."""

        section_path: Path = parent_path.joinpath(section.title + ".toml")

        with open(section_path, mode="w+", encoding="utf-8") as section_file:
            # This is for markdown, commented out.
            # heading = section.uri.rsplit(".", 1)
            # heading = section.uri
            # target = section.uri + "#" + heading[len(heading) - 1]

            # This is for rfc.
            target = section.uri
            requirements: list[dict] = []
            for requirement in section.requirements.values():
                temp_dict = {
                    TOML_REQ_LEVEL_KEY: requirement.requirement_level.name,
                    TOML_REQ_CONTENT_KEY: requirement.content,
                }
                requirements.append(temp_dict)

            section_toml = {TOML_URI_KEY: target, TOML_SPEC_KEY: requirements}
            toml.dump(section_toml, section_file)
        return [section_path]

    @staticmethod
    def _process_specification(specification: Specification, parent_path: Path) -> list[Path]:
        """Write TOML from Specification."""

        specification_path: Path = parent_path.joinpath(specification.title.split("#", 1)[0])
        specification_path.mkdir(exist_ok=True, parents=True)

        section_paths: list[Path] = []

        for section in specification.sections.values():
            section_paths.extend(TomlRequirementWriter._process_section(section, specification_path))
        return section_paths

    @staticmethod
    def process_report(report: Report, directory: Path) -> list[Path]:
        """Write TOML from Report."""

        section_paths: list[Path] = []
        for specification in report.specifications.values():
            section_paths.extend(TomlRequirementWriter._process_specification(specification, directory))

        return section_paths
