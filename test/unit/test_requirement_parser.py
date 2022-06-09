import pytest

from duvet.requirement_parser import (
    ALL_MARKDOWN_LIST_ENTRY_REGEX,
    ALL_RFC_LIST_ENTRY_REGEX,
    ListRequirements,
    create_requirements_from_list,
    extract_inline_requirements,
    extract_list_requirements,
    extract_requirements,
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
    "12. double digit\n"
    "something\n"
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

TEST_INVALID_STR = "A requirement MUST be terminated by one of the following\n\na. table\n1.) something\n"

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
    assert temp_list_req.list_elements == [
        "a. the document, ",
        "*  a cluster of documents [CLUSTER], and ",
        "-  the series of RFCs on the subject matter. ",
    ]
    # Verify the to_string_list function by checking the content of it creates.
    assert temp_list_req.to_string_list() == [
        "We MUST strive for consistency within: a. the document, ",
        "We MUST strive for consistency within: *  a cluster of documents [CLUSTER], and ",
        "We MUST strive for consistency within: -  the series of RFCs on the subject matter. ",
    ]


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
    assert req.list_elements == ["valid 1", "valid 2", "valid 3 This is something after valid 3"]
    assert ListRequirements.from_line(VALID_LIST_LINES).to_string_list() == [
        "This is a MUST requirement has lists valid 1",
        "This is a MUST requirement has lists valid 2",
        "This is a MUST requirement has lists valid 3 This is something after valid 3",
    ]


TEST_REQUIREMENT_STR = (
    "The specification section shows the specific specification text and how this links to annotation.\n"
    "It MUST show all text from the section. It MUST highlight the text for every requirement. It MUST hig"
    "hlight the text that matches any annotation. Any highlighted text MUST have a mouse ove"
    "r that shows its annotation information.\n"
    "Clicking on any highlighted text MUST bring up a popup that shows"
)


def test_extract_inline_requirements():
    assert extract_inline_requirements(TEST_REQUIREMENT_STR) == [
        "It MUST show all text from the section.",
        "It MUST highlight the text for every requirement.",
        "It MUST highlight the text that matches any annotation.",
        "Any highlighted text MUST have a mouse over that shows its annotation information.",
    ]


TEST_REQUIREMENT_STR_WITH_LIST = """Any complete sentence containing at least one \
RFC 2119 keyword MUST be treated as a requirement.
A requirement MAY contain multiple RFC 2119 keywords.
A requirement MUST be terminated by one of the following:

- period (.)
- exclamation point (!)
- list
- table

In the case of requirement terminated by a list,
the text proceeding the list MUST be concatenated
with each element of the list to form a requirement.
Taking the above list as an example,
Duvet is required to be able to recognize 4 different ways
to group text into requirements.
List elements MAY have RFC 2119 keywords,
this is the same as regular sentences with multiple keywords.
Sublists MUST be treated as if the parent item were terminated by the sublist.
List elements MAY contain a period (.) or exclamation point (!)
and this punctuation MUST NOT terminate the requirement by
excluding the following elements from the list of requirements.

In the case of requirement terminated by a table,
the text proceeding the table SHOULD be concatenated
with each row of the table to form a requirement.
Table cells MAY have RFC 2119 keywords,
this is the same as regular sentences with multiple keywords.
Table cells MAY contain a period (.) or exclamation point (!)
and this punctuation MUST NOT terminate the requirement
by excluding the following rows from the table of requirements.
"""


def test_extract_requirements():
    """Test Requirement without list."""
    assert extract_requirements(TEST_REQUIREMENT_STR) == [
        "It MUST show all text from the section.",
        "It MUST highlight the text for every requirement.",
        "It MUST highlight the text that matches any annotation.",
        "Any highlighted text MUST have a mouse over that shows its annotation information.",
    ]


def test_extract_requirements_with_lists_wrapped():
    """Test complicated requirement with list wrapped by inline requirements."""
    assert extract_requirements(TEST_REQUIREMENT_STR_WITH_LIST) == [  # pylint: disable=W1404
        "A requirement MAY contain multiple RFC 2119 keywords.",
        "A requirement MUST be terminated by one of the following: period (.)",
        "A requirement MUST be terminated by one of the following: exclamation point (!)",
        "A requirement MUST be terminated by one of the following: list",
        "A requirement MUST be terminated by one of the following: ",
        "List elements MAY have RFC 2119 keywords, this is the same as regular sentences with multiple keywords.",
        "Sublists MUST be treated as if the parent item were terminated by the sublist.",
        "List elements MAY contain a period (.) or exclamation point (!) and this "
        "punctuation MUST NOT terminate the requirement by excluding the following "
        "elements from the list of requirements.",
        "In the case of requirement terminated by a table, the text proceeding the "
        "table SHOULD be concatenated with each row of the table to form a "
        "requirement.",
        "Table cells MAY have RFC 2119 keywords, this is the same as regular sentences with multiple keywords.",
    ]


def test_extract_inline_requirements_complicated():
    """Test Complicated inline Requirement without list."""
    assert extract_inline_requirements(
        TEST_REQUIREMENT_STR_WITH_LIST[220: len(TEST_REQUIREMENT_STR_WITH_LIST) - 1]
    ) == [
               "List elements MAY have RFC 2119 keywords, this is the same as regular"
               " sentences with multiple keywords.",
               "Sublists MUST be treated as if the parent item were terminated by the sublist.",
               "List elements MAY contain a period (.) or exclamation point (!) and this "
               "punctuation MUST NOT terminate the requirement by excluding the following "
               "elements from the list of requirements.",
               "In the case of requirement terminated by a table, the text proceeding the "
               "table SHOULD be concatenated with each row of the table to form a requirement.",
               "Table cells MAY have RFC 2119 keywords, this is the same as"
               " regular sentences with multiple keywords.",
           ]
