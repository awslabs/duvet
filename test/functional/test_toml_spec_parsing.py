"""Specification Parser used by duvet-python for toml format."""

import pytest

from duvet.spec_toml_parser import extract_toml_specs
from ..utils import populate_file

pytestmark = [pytest.mark.local, pytest.mark.functional]

TEST_SPEC_TOML = """target = "../compliance/Users/yuancc/workspaces/duvet-python/spec/spec.txt#2.2.1"""""


TEST_SPEC_TOML_COMMENT ="""
# 2.2.1.  Section
#
# The top level header for requirements is the name of a section.  The
# name of the sections MUST NOT be nested.  A requirements section MUST
# be the top level containing header.  A header MUST NOT itself be a
# requirement.
# 
# A section MUST be indexable by combining different levels of naming.
# This means that Duvet needs to be able to locate it uniquely within a
# specification.  A good example of a section is a header in an HTML or
# Markdown document.
"""

TEST_SPEC_TOML_SPEC ="""
[[spec]]
level = "MUST"
quote = '''
The
name of the sections MUST NOT be nested.
'''

[[spec]]
level = "MUST"
quote = '''
A requirements section MUST
be the top level containing header.
'''

[[spec]]
level = "MUST"
quote = '''
A header MUST NOT itself be a
requirement.
'''

[[spec]]
level = "MUST"
quote = '''
A section MUST be indexable by combining different levels of naming.
'''
"""


def test_extreact_toml_spec():
    path = "./"
    patterns = "compliance/**/*.toml"
    test_report = extract_toml_specs(patterns, path)
    # Verify one spec is added to the report object
    assert len(test_report.specifications.keys()) == 1

def test_missing_keys(tmp_path):
    patterns = "compliance/**/*.toml"
    try:
        extract_toml_specs(patterns,populate_file(tmp_path, TEST_SPEC_TOML_COMMENT, "section1.toml"))
    except TypeError as error:
        # Verify the config function by checking the error message.
        assert repr(error) == ("ValueError('Specification Config not found.')")

    try:
        extract_toml_specs(patterns, populate_file(tmp_path, TEST_SPEC_TOML_SPEC, "section2.toml"))
    except ValueError as error:
        # Verify the config function by checking the error message.
        assert repr(error) == ("ValueError('Implementation Config not found.')")

    try:
        extract_toml_specs(patterns, populate_file(tmp_path, TEST_SPEC_TOML_SPEC, "section3.toml"))
    except ValueError as error:
        # Verify the config function by checking the error message.
        assert repr(error) == ("ValueError('Report Config not found.')")