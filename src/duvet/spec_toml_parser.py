# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Specification Parser used by duvet-python for toml format."""
import logging
import pathlib
import warnings
from pathlib import Path
from typing import Dict, List, Optional

import toml
from attrs import define

from duvet.identifiers import RequirementLevel
from duvet.structures import Report, Requirement, Section, Specification

__all__ = ["TomlRequirementParser"]

_LOGGER = logging.getLogger(__name__)

TOML_URI_KEY = "target"
TOML_SPEC_KEY = "spec"
TOML_REQ_LEVEL_KEY = "level"
TOML_REQ_CONTENT_KEY = "quotes"


@define
class TomlRequirementParser:
    """Parser for requirements in toml format."""

    @staticmethod
    def extract_toml_specs(patterns: str, path: pathlib.Path, toml_report: Optional[Report] = None) -> Report:
        """Take the patterns of the toml.

        Return a Report object containing all the specs.
        """
        # Because there are might be a lot of specs in this directory,
        # We will create a Report object to contain all the specs.
        if toml_report is None:
            toml_report = Report()
        for temp_toml in Path(path).glob(patterns):
            # Parse the attributes in section.
            sec_dict = toml.load(temp_toml)
            if sec_dict is None:
                warnings.warn(temp_toml.name + " is not a valid TOML file. Skipping file")
                continue
            uri = sec_dict.get(TOML_URI_KEY)
            if uri is None:
                warnings.warn(f'{temp_toml.name}: The key "{TOML_URI_KEY}" is missing. Skipping file.')
                continue
            title = uri.rsplit("#", 1)[-1]
            if title is None:
                warnings.warn(f'{temp_toml.name}: Could not process the key "{TOML_URI_KEY}". Skipping file.')
                continue
            spec_uri = uri.split("#")[0]
            # If the spec is not added to the dict yet. We add it to dict here.
            if spec_uri is None:
                warnings.warn(f'{temp_toml.name}: Could not process the key "{TOML_URI_KEY}". Skipping file.')
                continue
            if toml_report.specifications.get(spec_uri) is None:
                toml_report.specifications[spec_uri] = Specification(spec_uri.split("/")[1], spec_uri)
            temp_sec = Section(title, uri)
            requirements = sec_dict.get(TOML_SPEC_KEY)
            if requirements is not None:
                _parse_requirement_attributes(requirements, sec_dict, temp_sec)
            toml_report.specifications.get(spec_uri).add_section(temp_sec)

        return toml_report


def _parse_requirement_attributes(requirements: List[Requirement], sec_dict: Dict, temp_sec: Section):
    # Parse the attributes in Requirement.
    for req in requirements:
        temp_req = Requirement(
            RequirementLevel[req.get(TOML_REQ_LEVEL_KEY)], req.get(TOML_REQ_CONTENT_KEY), sec_dict.get(TOML_URI_KEY)
        )
        temp_sec.add_requirement(temp_req)
