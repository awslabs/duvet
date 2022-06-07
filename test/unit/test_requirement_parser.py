import pytest

from duvet.identifiers import *
from duvet.requirement_parser import *
from duvet.structures import *

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
    "We MUST strive for consistency within:\n"
    "\n"
    "      a. the document,\n"
    "\n"
    "      *  a cluster of documents [CLUSTER], and\n"
    "\n"
    "      -  the series of RFCs on the subject matter.\n"
    "\n"
    "+  plus\n"
    "1.) something\n"
    "+ plus\n"
)

TEST_INVALID_STR = "A requirement MUST be terminated by one of the following\n" "\n" "a. table\n" "1.) something" "\n"


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


def test_create_requirement_from_list():
    lines = TEST_VALID_MARKDOWN_LIST.splitlines()
    temp_list_req = extract_list_requirements(lines, 0, 8, ALL_MARKDOWN_LIST_ENTRY_REGEX)
    test_sec = Section("A Section Title", "h1.h2.h3.a-section-title", 1, 3)
    assert test_sec.title == "A Section Title"
    assert test_sec.uri == "h1.h2.h3.a-section-title"
    assert (
        test_sec.to_github_url("spec/spec.md", "https://github.com/awslabs/duvet")
        == "https://github.com/awslabs/duvet/blob/master/spec/spec.md#a-section-title"
    )
    assert not test_sec.has_requirements
    assert create_requirements_from_list(test_sec, temp_list_req)
    assert test_sec.has_requirements
    # Verify the extract_list function by checking the number of requirements it adds to section
    assert len(test_sec.requirements.keys()) == 5
