# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Functional testing for requirement parsing"""

import pytest

# from duvet._config import Config, ImplConfig
#
# from ..utils import populate_file  # isort:skip
from duvet.spec_md_parser import MDSpec

pytestmark = [pytest.mark.local, pytest.mark.functional]

def test_extract_python_no_implementation_annotation(pytestconfig):
    path = pytestconfig.rootpath.joinpath("duvet-specification", "duvet-specification.md")
    print(MDSpec.load(path))
