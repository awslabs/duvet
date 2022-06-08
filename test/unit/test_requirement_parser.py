import re

import pytest

from duvet.requirement_parser import (
    ALL_MARKDOWN_LIST_ENTRY_REGEX,
    ALL_RFC_LIST_ENTRY_REGEX,
    create_requirements_from_list,
    extract_list_requirements, FIND_ALL_MARKDOWN_LIST_ELEMENT_REGEX,
    ListRequirements
)
from duvet.structures import Section

pytestmark = [pytest.mark.unit, pytest.mark.local]

TEST_VALID_MARKDOWN_LIST = (
    "A requirement MUST be terminated by one of the following\n"
    "\n"
    "* period (.)\n"
    "- exclamation point (!)\n"
    "+  plus\n"
    "1. list\n"
    "something\n"
    # "a. table\n"
    "12. double digit\n"
    "something\n"
    # "1.) something"
    "\n"
)

TEST_RFC_STR = (
    "We MUST strive for consistency within:\n"  # Valid RFC List Parent
    "+  plus\n"  # Invalid RFC list
    "1.) something\n"  # Invalid RFC list
    "+ plus\n"  # Invalid RFC list
    "\n"
    "      a. the document,\n"  # Valid RFC list
    "\n"
    "      *  a cluster of documents [CLUSTER], and\n"  # Valid RFC list
    "\n"
    "      -  the series of RFCs on the subject matter.\n"  # Valid RFC list
    "\n"
)

TEST_INVALID_STR = 'A requirement MUST be terminated by one of the following\n\na. table\n1.) something\n'

TEST_VALID_WRAPPED_MARKDOWN_LIST = (
    "A requirement MUST be terminated by one of the following\n"
    "\n"
    "* period (.)\n"
    "* exclamation point (!)\n"
    "*  plus\n"
    "1. list\n"
    "something\n"
    # "a. table\n"
    "12. double digit\n"
    "something\n"
    # "1.) something"
    "\n"
)


def test_extract_valid_md_list():
    lines = TEST_VALID_MARKDOWN_LIST.splitlines()
    temp_list_req = extract_list_requirements(lines, 0, 8, ALL_MARKDOWN_LIST_ENTRY_REGEX)
    assert temp_list_req.list_parent == "A requirement MUST be terminated by one of the following"
    # Verify the extract_list function by checking the number of children it extracts
    assert len(temp_list_req.list_elements) == 5
    assert temp_list_req.list_elements == [
        "* period (.)",
        "- exclamation point (!)",
        "+  plus",
        "1. list something",
        "12. double digit something",
    ]


def test_extract_invalid_md_list():
    lines = TEST_INVALID_STR.splitlines()
    temp_list_req = extract_list_requirements(lines, 0, 3, ALL_MARKDOWN_LIST_ENTRY_REGEX)
    assert temp_list_req.list_parent == "A requirement MUST be terminated by one of the following"
    # Verify the extract_list function by checking the number of children it extracts
    assert len(temp_list_req.list_elements) == 0


def test_extract_rfc_list():
    lines = TEST_RFC_STR.splitlines()
    temp_list_req = extract_list_requirements(lines, 0, 10, ALL_RFC_LIST_ENTRY_REGEX)
    assert temp_list_req.list_parent == "We MUST strive for consistency within:"
    # Verify the extract_list function by checking the number of children it extracts
    assert len(temp_list_req.list_elements) == 3
    assert temp_list_req.list_elements == ['a. the document, ',
                                           '*  a cluster of documents [CLUSTER], and ',
                                           '-  the series of RFCs on the subject matter. ']


def test_create_requirement_from_list():
    lines = TEST_VALID_MARKDOWN_LIST.splitlines()
    temp_list_req = extract_list_requirements(lines, 0, 8, ALL_MARKDOWN_LIST_ENTRY_REGEX)
    test_sec = Section("A Section Title", "h1.h2.h3.a-section-title", 1, 3)
    assert test_sec.title == "A Section Title"
    assert test_sec.uri == "h1.h2.h3.a-section-title"
    temp_str = test_sec.to_github_url("spec/spec.md", "https://github.com/awslabs/duvet")
    assert temp_str == "https://github.com/awslabs/duvet/blob/master/spec/spec.md#a-section-title"
    assert not test_sec.has_requirements
    assert create_requirements_from_list(test_sec, temp_list_req)
    assert test_sec.has_requirements
    # Verify the extract_list function by checking the number of requirements it adds to section
    assert len(test_sec.requirements.keys()) == 5


VALID_LIST_LINES = """This is a MUST requirement has lists
* valid 1
* valid 2
* valid 3
This is something after valid 3

This is a sentence after the list"""


def test_search():
    req = ListRequirements.from_line(VALID_LIST_LINES)
    assert req.list_parent == "This is a MUST requirement has lists"
    assert req.list_elements == ['valid 1', 'valid 2', 'valid 3 This is something after valid 3']
