import pytest

from duvet.identifiers import *
from duvet.requirement_parser import *
from duvet.structures import *

pytestmark = [pytest.mark.unit, pytest.mark.local]

TEST_STR = (
    "A requirement MUST be terminated by one of the following\n"
    "\n"
    "* period (.)\n"
    "- exclamation point (!)\n"
    "1. list\n"
    "a. table\n"
    "12. double digit\n"
    "*  double space\n"
    "\n"
)


def test_extract_list():
    lines = TEST_STR.splitlines()
    temp_list_req = extract_list_requirements(lines, 0, 8)
    assert temp_list_req.list_parent == "A requirement MUST be terminated by one of the following"
    # Verify the extract_list function by checking the number of children it extracts
    assert len(temp_list_req.list_elements) == 6
    print(temp_list_req.list_elements)


def test_create_requirement_from_list():
    lines = TEST_STR.splitlines()
    temp_list_req = extract_list_requirements(lines, 0, 8)
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
    assert len(test_sec.requirements.keys()) == 6
