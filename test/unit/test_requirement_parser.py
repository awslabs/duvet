# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Unit tests for ``duvet.requirement_parser``."""
import pytest

from duvet.identifiers import RequirementLevel
from duvet.requirement_parser import ALL_MARKDOWN_LIST_ENTRY_REGEX, ALL_RFC_LIST_ENTRY_REGEX, RequirementParser
from duvet.specification_parser import Span
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
    "\n"
    "      a. the document,\n"  # Valid RFC list
    "\n"
    "      *  a cluster of documents [CLUSTER], and\n"  # Valid RFC list
    "\n"
    "      -  the series of RFCs on the subject matter.\n"  # Valid RFC list
    "\n"
)

# "+  plus\n"  # Invalid RFC list
# "1.) something\n"  # Invalid RFC list
# "+ plus\n"  # Invalid RFC list

TEST_INVALID_STR = "A requirement MUST be terminated by one of the following\n\na. table\n1.) something\n"

TEST_VALID_WRAPPED_MARKDOWN_LIST = (
    "A requirement MUST be terminated by one of the following\n"
    "\n"
    "* period (.)\n"
    "* exclamation point (!)\n"
    "*  plus\n"
    "1. list\n"
    "something\n"
    "12. double digit\n"
    "something\n"
    "\n"
)


def test_extract_valid_md_list():
    test_parser = RequirementParser(TEST_VALID_MARKDOWN_LIST)
    test_span = Span(0, len(TEST_VALID_MARKDOWN_LIST))
    temp_list_req: dict = test_parser.process_list_block(test_span)[0]
    # Verify the extract_list function by checking the number of children it extracts
    assert temp_list_req.get("parent") == Span(start=0, end=58)
    assert len(temp_list_req.get("children")) == 5
    assert temp_list_req.get("children") == [
        Span(start=60, end=71),
        Span(start=73, end=95),
        Span(start=97, end=103),
        Span(start=106, end=121),
        Span(start=125, end=149),
    ]


# Pass
def test_extract_invalid_md_list():
    test_parser = RequirementParser(TEST_INVALID_STR)
    test_span = Span(0, len(TEST_INVALID_STR))
    try:
        test_parser.process_list_block(test_span)
    except ValueError as error:
        # Verify the extract_list function by checking the error message.
        assert repr(error) == (
            "ValueError('Requirement list syntax is not valid in A requirement MUST be "
            "terminated by one of the following\\n\\na. table\\n1.) something\\n')"
        )


# def test_extract_rfc_list():
#     temp_list_req = ListRequirements.from_line(TEST_RFC_STR, ALL_RFC_LIST_ENTRY_REGEX)
#     assert temp_list_req.list_parent == "We MUST strive for consistency within:"
#     # Verify the extract_list function by checking the number of children it extracts
#     assert len(temp_list_req.list_elements) == 3
#     assert temp_list_req.list_elements == [
#         "the document,",
#         " a cluster of documents [CLUSTER], and",
#         " the series of RFCs on the subject matter.",
#     ]
#     # Verify the to_string_list function by checking the content of it creates.
#     assert temp_list_req.to_string_list(False) == [
#         "We MUST strive for consistency within: the document,",
#         "We MUST strive for consistency within:  a cluster of documents [CLUSTER], and",
#         "We MUST strive for consistency within:  the series of RFCs on the subject matter.",
#     ]


VALID_LIST_LINES = """This is a MUST requirement has lists
* valid 1
* valid 2
* valid 3
This is something after valid 3

This is a sentence after the list"""


def test_process_list():
    actual_parser = RequirementParser(TEST_VALID_MARKDOWN_LIST)
    actual_dict = {
        "parent": Span(start=0, end=58),
        "children": [
            Span(start=60, end=71),
            Span(start=73, end=95),
            Span(start=97, end=103),
            Span(start=106, end=121),
            Span(start=125, end=149),
        ],
    }
    # default
    req = actual_parser.process_list(actual_dict)
    assert req == [
        {
            "content": "A requirement MUST be terminated by one of the following period " "(.)",
            "requirement_level": RequirementLevel.MUST,
            "span": Span(start=60, end=71),
        },
        {
            "content": "A requirement MUST be terminated by one of the following " "exclamation point (!)",
            "requirement_level": RequirementLevel.MUST,
            "span": Span(start=73, end=95),
        },
        {
            "content": "A requirement MUST be terminated by one of the following plus",
            "requirement_level": RequirementLevel.MUST,
            "span": Span(start=97, end=103),
        },
        {
            "content": "A requirement MUST be terminated by one of the following list " "something",
            "requirement_level": RequirementLevel.MUST,
            "span": Span(start=106, end=121),
        },
        {
            "content": "A requirement MUST be terminated by one of the following double " "digit something",
            "requirement_level": RequirementLevel.MUST,
            "span": Span(start=125, end=149),
        },
    ]


TEST_REQUIREMENT_STR = "Something something.\n" "Duvet MUST implement " "every requirement. " "Something something.\n"

TEST_REQUIREMENT_WITH_INVALID_STR = (
    "Something something.\n"
    "Duvet MUST implement"
    "every requirement e.g. this is an example try to break parser."
    "Something something.\n"
)


@pytest.fixture
def under_test(tmp_path) -> RequirementParser:
    return RequirementParser(tmp_path)


def test_process_inline():
    actual_parser = RequirementParser(TEST_REQUIREMENT_STR)
    actual_span = Span(0, len(TEST_REQUIREMENT_STR) - 1)

    # Test valid inline text
    assert actual_parser.process_inline(actual_span) == [
        {
            "content": "Duvet MUST implement every requirement.",
            "requirement_level": RequirementLevel.MUST,
            "span": Span(start=28, end=74),
        }
    ]


TEST_REQUIREMENT_STR_WITH_LIST = """A requirement MAY contain multiple RFC 2119 keywords.
A requirement SHOULD be terminated by one of the following:

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
"""


def test_extract_requirements_with_lists_wrapped():
    """Test complicated requirement with list wrapped by inline requirements."""
    actual_parser = RequirementParser(TEST_REQUIREMENT_STR_WITH_LIST)
    actual_spans = actual_parser.extract_block(Span(0, len(TEST_REQUIREMENT_STR_WITH_LIST)))
    assert actual_spans == [
        (Span(start=0, end=54), "INLINE"),
        (Span(start=54, end=168), "LIST_BLOCK"),
        (Span(start=168, end=449), "INLINE"),
    ]

    actual_kwargs = actual_parser.extract_requirements(actual_spans)
    assert actual_kwargs == [
        {
            "content": "A requirement MAY contain multiple RFC 2119 keywords.",
            "requirement_level": RequirementLevel.MAY,
            "span": Span(start=0, end=61),
        },
        "parent",
        "children",
        {
            "content": "In the case of requirement terminated by a list, the text "
                       "proceeding the list MUST be concatenated with each element of "
                       "the list to form a requirement.",
            "requirement_level": RequirementLevel.MUST,
            "span": Span(start=168, end=327),
        },
    ]
