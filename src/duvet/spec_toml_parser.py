# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Specification Parser used by duvet-python for toml format."""
import logging
import os
import warnings
from pathlib import Path

import toml

__all__ = ["Config"]

from duvet.identifiers import RequirementLevel
from duvet.structures import Report, Requirement, Section, Specification

_LOGGER = logging.getLogger(__name__)


def extract_toml_specs(patterns: str, path: str) -> Report:
    """Take the patterns of the toml.

    Return a Report object containing all the specs.
    """
    try:
        os.chdir(path)
        _LOGGER.warning(f"Current working directory: {0}".format(os.getcwd()))
    except FileNotFoundError:
        warnings.warn(f"Directory: {0} does not exist".format(path))
    except NotADirectoryError:
        warnings.warn(f"{0} is not a directory".format(path))
    except PermissionError:
        warnings.warn(f"You do not have permissions to change to {0}".format(path))
    toml_list = Path().glob(patterns)
    temp_toml_list = list(toml_list)
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
            req_uri = uri
            req_content = req.get("quote")
            req_level_name = req.get("level")
            req_level = RequirementLevel[req_level_name]
            temp_req = Requirement(req_level, req_content, req_uri)
            temp_sec.add_requirement(temp_req)
        spec_uris[spec_uri].add_section(temp_sec)

    for temp_spec in spec_uris.values():
        toml_report.add_specification(temp_spec)

    return toml_report
