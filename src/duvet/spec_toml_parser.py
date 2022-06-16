# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Specification Parser used by duvet-python for toml format."""
import logging
import pathlib
from pathlib import Path

import toml

from duvet.identifiers import RequirementLevel
from duvet.structures import Report, Requirement, Section, Specification

_LOGGER = logging.getLogger(__name__)


def extract_toml_specs(patterns: str, path: pathlib.Path) -> Report:
    """Take the patterns of the toml.

    Return a Report object containing all the specs.
    """
    temp_toml_list = list(Path(path).glob(patterns))
    # Because there are might be a lot of specs in this directory,
    # We will create a Report object to contain all the specs.
    toml_report = Report()
    spec_uris = {}

    for temp_toml in temp_toml_list:
        # Parse the attributes in section.
        sec_dict = toml.load(temp_toml)
        uri = sec_dict.get("target")
        title = uri.split("#")[1]
        spec_uri = uri.split("#")[0]
        if spec_uris.get(spec_uri) is None:
            spec_uris[spec_uri] = Specification(spec_uri.split("/")[1], spec_uri)
        temp_sec = Section(title, uri)
        requirements = sec_dict.get("spec")
        # Parse the attributes in Requirement.
        for req in requirements:
            # req_uri = uri
            # req_content = req.get("quote")
            # req_level_name = req.get("level")
            # req_level = RequirementLevel[req.get("level")]
            temp_req = Requirement(RequirementLevel[req.get("level")], req.get("quote"), uri)
            temp_sec.add_requirement(temp_req)
        spec_uris.get(spec_uri).add_section(temp_sec)

    for temp_spec in spec_uris.values():
        toml_report.add_specification(temp_spec)

    return toml_report
