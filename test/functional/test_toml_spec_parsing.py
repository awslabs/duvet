"""Specification Parser used by duvet-python for toml format."""

import pytest

from duvet.spec_toml_parser import TomlRequirementParser

from ..utils import populate_file  # isort:skip

pytestmark = [pytest.mark.local, pytest.mark.functional]

TEST_SPEC_TOML_TARGET = """target = "../duvet-python/spec/spec.txt#2.2.1"    """

TEST_SPEC_TOML_COMMENT = """
# 2.2.1.  Section
#
# The top level header for requirements is the name of a section.  The
# name of the sections MUST NOT be nested.  A requirements section MUST
# be the top level containing header.  A header MUST NOT itself be a
# requirement.
# A section MUST be indexable by combining different levels of naming.
# This means that Duvet needs to be able to locate it uniquely within a
# specification.  A good example of a section is a header in an HTML or
# Markdown document.
"""

TEST_SPEC_TOML_SPEC = """
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
    test_report = TomlRequirementParser.extract_toml_specs(patterns, path)
    # Verify one spec is added to the report object
    assert len(test_report.specifications.keys()) == 1


def test_missing_uri(tmp_path):
    # We will not throw error is there is no targset.
    patterns = "compliance/**/*.toml"
    populate_file(tmp_path, TEST_SPEC_TOML_COMMENT, "compliance/spec/section1.toml")
    with pytest.warns(UserWarning) as record:
        TomlRequirementParser.extract_toml_specs(patterns, tmp_path)
    # print(UserWarning.)
    # check that only one warning was raised
    assert len(record) == 1
    # check that the message matches
    assert record[0].message.args[0] == 'section1.toml: The key "target" is missing. Skipping file.'


def test_missing_specs(tmp_path):
    # We will not throw error is there is no requirements.
    patterns = "compliance/**/*.toml"
    populate_file(tmp_path, TEST_SPEC_TOML_TARGET, "compliance/spec/section1.toml")
    actual_report = TomlRequirementParser.extract_toml_specs(patterns, tmp_path)
    # Verify one section is added to the report object
    assert (
        actual_report.specifications.get("../duvet-python/spec/spec.txt")
        .sections.get("../duvet-python/spec/spec.txt#2.2.1")
        .requirements
        == {}
    )


def test_extract_spec_toml(tmp_path):
    # We will not throw error is there is no requirements.
    patterns = "compliance/**/*.toml"
    populate_file(tmp_path, "\n".join([TEST_SPEC_TOML_TARGET, TEST_SPEC_TOML_SPEC]), "compliance/spec/section1.toml")
    actual_report = TomlRequirementParser().extract_toml_specs(patterns, tmp_path)
    # Verify requirements is added to the report object
    assert (
        len(
            actual_report.specifications.get("../duvet-python/spec/spec.txt")
            .sections.get("../duvet-python/spec/spec.txt#2.2.1")
            .requirements
        )
        == 1
    )
