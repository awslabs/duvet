# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Specification Parser used by duvet-python for toml format."""
from pathlib import Path
import os

import toml

from duvet.identifiers import RequirementLevel
from duvet.structures import Section, Requirement, Report, Specification

path = '../../'

try:
    os.chdir(path)
    print("Current working directory: {0}".format(os.getcwd()))
except FileNotFoundError:
    print("Directory: {0} does not exist".format(path))
except NotADirectoryError:
    print("{0} is not a directory".format(path))
except PermissionError:
    print("You do not have permissions to change to {0}".format(path))

patterns = "compliance/**/*.toml"

toml_list = Path().glob(patterns)
temp_toml = list(toml_list)

rp = Report()
spec_uris = {}

for tt in temp_toml:
    # Parse the attributes in section.
    sec_dict = toml.load(tt)
    uri = sec_dict["target"]
    title = uri.split("#")[1]
    spec_uri = uri.split("#")[0]
    if spec_uri not in spec_uris.keys():
        spec_uris[spec_uri] = Specification(spec_uri.split("/")[1],spec_uri)
    temp_sec = Section(title, uri)
    requirements = sec_dict["spec"]
    # Parse the attributes in Requirement.
    for req in requirements:
        req_uri = uri
        req_content = req["quote"]
        req_level_name = req["level"]
        req_level = RequirementLevel[req_level_name]
        temp_req = Requirement(req_level, req_content, req_uri)
        temp_sec.add_requirement(temp_req)
    # print(temp_sec)
    spec_uris[spec_uri].add_section(temp_sec)

for temp_spec in spec_uris.values():
    rp.add_specification(temp_spec)

print(rp)