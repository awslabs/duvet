# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Unit tests for ``duvet.requirement_parser``."""
import pytest

from duvet.identifiers import RequirementLevel, ALL_MARKDOWN_LIST_ENTRY_REGEX
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


class TestMarkdownProcessList:

    @staticmethod
    def test_extract_valid_md_list():
        actual_span = Span(0, len(TEST_VALID_MARKDOWN_LIST))
        test_parser = RequirementParser._process_list_block(TEST_VALID_MARKDOWN_LIST,
                                                            actual_span,
                                                            ALL_MARKDOWN_LIST_ENTRY_REGEX)

        actual_list_requirement_dict: dict = test_parser[0]

        # Verify the extract_list function by checking the number of children it extracts
        assert actual_list_requirement_dict.get("parent") == Span(start=0, end=58)
        assert len(actual_list_requirement_dict.get("children")) == 5
        assert actual_list_requirement_dict.get("children") == [
            Span(start=60, end=71),
            Span(start=73, end=95),
            Span(start=97, end=103),
            Span(start=106, end=121),
            Span(start=125, end=149),
        ]

    @staticmethod
    def test_extract_invalid_md_list():
        actual_span = Span(0, len(TEST_INVALID_STR))
        test_parser = RequirementParser._process_list_block(TEST_INVALID_STR,
                                                            actual_span,
                                                            ALL_MARKDOWN_LIST_ENTRY_REGEX)
        assert not test_parser


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


class TestComplicatedRequirements:

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
