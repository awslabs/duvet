# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Specification Parser used by duvet-python for toml format."""
import logging
import re
import warnings
from pathlib import Path
from typing import Any, Dict, List, MutableMapping, Optional

import toml
from attrs import define, field

from duvet.formatter import clean_content
from duvet.identifiers import RequirementLevel
from duvet.structures import Report, Requirement, Section, Specification

_LOGGER = logging.getLogger(__name__)
__all__ = ["TomlRequirementParser"]

TOML_URI_KEY: str = "target"
TOML_SPEC_KEY: str = "spec"
TOML_REQ_LEVEL_KEY: str = "level"
TOML_REQ_CONTENT_KEY: str = "quote"


@define
class TomlRequirementParser:
    """Parser for requirements in toml format."""

    toml_path: Path = field(init=False)

    def extract_toml_specs(self, patterns: str, path: Path, toml_report: Optional[Report] = None) -> Report:
        """Take the patterns of the toml.

        Return a Report object containing all the specs.
        """
        # Because there are might be a lot of specs in this directory,
        # We will create a Report object to contain all the specs.
        if toml_report is None:
            toml_report = Report()
        for temp_toml in Path(path).glob(patterns):
            # Parse the attributes in section.

            sec_dict: Dict = toml.load(temp_toml)
            if sec_dict is None:
                warnings.warn(str(temp_toml.resolve()) + " is not a valid TOML file. Skipping file")
                continue

            section_uri = sec_dict.get(TOML_URI_KEY)
            if section_uri is None:
                warnings.warn(f'{str(temp_toml.resolve())}: The key "{TOML_URI_KEY}" is missing. Skipping file.')
                continue
            section_uri = clean_content(section_uri)

            title = section_uri.rsplit("#", 1)[-1]
            if title is None:
                warnings.warn(f'{str(temp_toml.resolve())}: Could not process the key "{TOML_URI_KEY}". Skipping file.')
                continue
            title = clean_content(title)

            spec_uri = section_uri.rsplit("#", 1)[0]
            # If the spec is not added to the dict yet. We add it to dict here.
            if spec_uri is None:
                warnings.warn(f'{str(temp_toml.resolve())}: Could not process the key "{TOML_URI_KEY}". Skipping file.')
                continue
            spec_uri = clean_content(spec_uri)

            if toml_report.specifications.get(spec_uri) is None:
                toml_report.specifications[spec_uri] = Specification(spec_uri.rsplit("/", maxsplit=1)[-1], spec_uri)

            temp_sec = Section(title, section_uri)

            # Parse lines from legacy toml files
            with open(temp_toml, mode="r", encoding="utf-8") as section_toml:
                lines = section_toml.readlines()
                lines = [clean_content(line) for line in lines if re.search(r"^#", line) is not None]
                temp_sec.lines = lines

            requirements = sec_dict.get(TOML_SPEC_KEY)
            if requirements is not None:
                self.toml_path = temp_toml
                self._parse_requirement_attributes(requirements, sec_dict, temp_sec)
            # TODO: use a default dict for Report.specifications  # pylint: disable=fixme
            toml_report.specifications.get(spec_uri).add_section(temp_sec)  # type: ignore[union-attr]

        return toml_report

    def _parse_requirement_attributes(
        self,
        requirements: List[MutableMapping[str, Any]],
        sec_dict: MutableMapping[str, Any],
        temp_sec: Section,
    ):
        # Parse the attributes in Requirement.
        # TODO: refactor to class method to grant access to filepath via self  # pylint: disable=fixme
        for req in requirements:
            try:
                level: str = req.get(TOML_REQ_LEVEL_KEY)  # type: ignore[assignment] # will raise AttributeError
                content: str = clean_content(
                    req.get(TOML_REQ_CONTENT_KEY)  # type: ignore[arg-type] # will raise AttributeError
                )
                toml_uri: str = clean_content(
                    sec_dict.get(TOML_URI_KEY)  # type: ignore[arg-type] # will raise AttributeError
                )
                temp_req = Requirement(
                    RequirementLevel[level],  # will raise KeyError
                    content,
                    "$".join([toml_uri, content]),  # type: ignore[list-item] # will raise AttributeError
                )
                temp_sec.add_requirement(temp_req)
            except (TypeError, KeyError, AttributeError) as ex:
                _LOGGER.info("%s: Failed to parse %s into a Requirement.", (str(self.toml_path.resolve()), req), ex)
