# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Specification TOML writer used by duvet-python for toml format.

Assumptions:
1. TODO: Need to write lines when the HTML PR is merged.
"""
import logging
from pathlib import Path

import tomli_w  # type:ignore[import]
from attr import define

from duvet.identifiers import TOML_REQ_CONTENT_KEY, TOML_REQ_LEVEL_KEY, TOML_SPEC_KEY, TOML_URI_KEY
from duvet.structures import Report, Section, Specification

_LOGGER = logging.getLogger(__name__)
__all__ = ["TomlRequirementWriter"]


@define
class TomlRequirementWriter:
    """TOML specifications writer."""

    @staticmethod
    def _process_section(section: Section, parent_path: Path, file_type: str = "RFC") -> list[Path]:
        """Write TOML from Section."""

        section_path: Path = parent_path.joinpath(section.title + ".toml")

        with open(section_path, mode="wb") as section_file:
            if file_type == "MARKDOWN":
                target = section.uri + "#" + section.uri.rsplit(".", 1)[1]

            # Process rfc title.
            if file_type == "RFC":
                target = section.uri

            requirements: list[dict] = []
            for requirement in section.requirements.values():
                temp_dict = {
                    TOML_REQ_LEVEL_KEY: requirement.requirement_level.name,
                    TOML_REQ_CONTENT_KEY: requirement.content,
                }
                requirements.append(temp_dict)

            section_toml = {TOML_URI_KEY: target, TOML_SPEC_KEY: requirements}
            tomli_w.dump(section_toml, section_file)
        return [section_path]

    @staticmethod
    def _process_specification(specification: Specification, parent_path: Path, file_type: str = "RFC") -> list[Path]:
        """Write TOML from Specification."""

        specification_path: Path = parent_path.joinpath(specification.title.split("#", 1)[0])
        specification_path.mkdir(exist_ok=True, parents=True)

        section_paths: list[Path] = []

        for section in specification.sections.values():
            section_paths.extend(TomlRequirementWriter._process_section(section, specification_path, file_type))
        return section_paths

    @staticmethod
    def process_report(report: Report, directory: Path, file_type: str = "RFC") -> list[Path]:
        """Write TOML from Report."""

        section_paths: list[Path] = []
        for specification in report.specifications.values():
            section_paths.extend(TomlRequirementWriter._process_specification(specification, directory, file_type))

        return section_paths
