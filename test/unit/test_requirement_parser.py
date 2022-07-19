# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Unit tests for ``duvet.requirement_parser``."""
import pytest

from duvet.formatter import clean_content
from duvet.identifiers import ALL_MARKDOWN_LIST_ENTRY_REGEX, ALL_RFC_LIST_ENTRY_REGEX, RequirementLevel
from duvet.requirement_parser import RequirementParser
from duvet.specification_parser import Span

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

VALID_LIST_LINES = """This is a MUST requirement has lists
* valid 1
* valid 2
* valid 3
This is something after valid 3

This is a sentence after the list"""

TEST_REQUIREMENT_STR = "Something something.\nDuvet MUST implement every requirement. Something something.\n"

TEST_REQUIREMENT_WITH_INVALID_STR = (
    "Something something.\n"
    "Duvet MUST implement"
    "every requirement e.g. this is an example try to break parser."
    "Something something.\n"
)

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


class TestProcessList:
    @staticmethod
    def test_extract_valid_md_list():
        actual_span = Span(0, len(TEST_VALID_MARKDOWN_LIST))
        actual_list_requirement: list = RequirementParser._process_list_block(
            TEST_VALID_MARKDOWN_LIST, actual_span, ALL_MARKDOWN_LIST_ENTRY_REGEX
        )
        # Verify the extract_list function by checking the dictionary it extracts
        expected_list_requirement = {
            "children": [
                Span(60, 71),
                Span(73, 95),
                Span(97, 103),
                Span(106, 121),
                Span(125, 149),
            ],
            "parent": Span(0, 58),
        }

        assert actual_list_requirement[0] == expected_list_requirement

    @staticmethod
    def test_process_invalid_md_list():
        test_span = Span(0, len(TEST_INVALID_STR))
        actual_list_requirement: list = RequirementParser._process_list_block(
            TEST_INVALID_STR, test_span, ALL_MARKDOWN_LIST_ENTRY_REGEX
        )
        assert not actual_list_requirement

    @staticmethod
    def test_process_rfc_list():
        quote_span = Span(0, len(TEST_RFC_STR))
        temp_list_req = RequirementParser._process_list_block(TEST_RFC_STR, quote_span, ALL_RFC_LIST_ENTRY_REGEX)

        actual_span = temp_list_req[0]["parent"]
        assert (
            clean_content(TEST_RFC_STR[actual_span.start : actual_span.end]) == "We MUST strive for consistency within:"
        )

        # Verify the extract_list function by checking the number of children it extracts
        children = temp_list_req[0].get("children")

        assert len(children) == 3

        list_req = [clean_content(TEST_RFC_STR[child.start : child.end]) for child in children]

        assert list_req == [
            "the document,",
            "a cluster of documents [CLUSTER], and",
            "the series of RFCs on the subject matter.",
        ]

        # Verify the to_string_list function by checking the content of it creates.
        assert [
            req.get("content") for req in RequirementParser._process_list(TEST_RFC_STR, temp_list_req[0], False)
        ] == [
            "We MUST strive for consistency within: the document,",
            "We MUST strive for consistency within: a cluster of documents [CLUSTER], and",
            "We MUST strive for consistency within: the series of RFCs on the subject matter.",
        ]

    @staticmethod
    def test_process_list():
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
        req = RequirementParser._process_list(TEST_VALID_MARKDOWN_LIST, actual_dict, False)

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


class TestProcessInline:
    @staticmethod
    def test_process_inline():
        actual_span = Span(0, len(TEST_REQUIREMENT_STR))

        # Test valid inline text
        assert RequirementParser._process_inline(TEST_REQUIREMENT_STR, actual_span) == [
            {
                "content": "Duvet MUST implement every requirement.",
                "requirement_level": RequirementLevel.MUST,
                "span": Span(21, end=61),
            }
        ]

    @staticmethod
    def test_extract_requirements_with_lists_wrapped():
        """Test complicated requirement with list wrapped by inline requirements."""

        quote_span = Span(0, len(TEST_REQUIREMENT_STR_WITH_LIST))
        actual_spans = RequirementParser._process_block(
            TEST_REQUIREMENT_STR_WITH_LIST, quote_span, ALL_MARKDOWN_LIST_ENTRY_REGEX
        )
        assert actual_spans == [
            (Span(start=0, end=54), "INLINE"),
            (Span(start=54, end=168), "LIST_BLOCK"),
            (Span(start=168, end=449), "INLINE"),
        ]

        actual_kwargs = RequirementParser._process_section(
            TEST_REQUIREMENT_STR_WITH_LIST, actual_spans, ALL_MARKDOWN_LIST_ENTRY_REGEX
        )
        expected_content = [
            "A requirement MAY contain multiple RFC 2119 keywords.",
            "A requirement SHOULD be terminated by one of the following: period (.)",
            "A requirement SHOULD be terminated by one of the following: exclamation point (!)",
            "A requirement SHOULD be terminated by one of the following: list",
            "A requirement SHOULD be terminated by one of the following: table",
            "In the case of requirement terminated by a list, the text "
            "proceeding the list MUST be concatenated with each element of "
            "the list to form a requirement.",
        ]

        actual_content = [kwargs["content"] for kwargs in actual_kwargs]
        assert actual_content == expected_content
