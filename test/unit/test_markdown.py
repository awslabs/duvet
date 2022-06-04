# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Unit test suite for duvet.markdown."""
from typing import TypeVar

import pytest

from duvet.markdown import MAX_HEADER_LEVELS, MarkdownHeader, MarkdownSpecification

pytestmark = [pytest.mark.unit, pytest.mark.local]

HEADER_POSITIVE_CASES = {
    "# Duvet Specification": (1, "Duvet Specification"),
    "## Overview !@#$%^&*()_+": (2, "Overview !@#$%^&*()_+"),
}
HEADER_NEGATIVE_CASES = [
    "#",
    "#  ",
    "#\n",
    "#\t",
    "#\r",
    "#\f",
    "#\v",
    "".join(["#" for i in range(0, MAX_HEADER_LEVELS)]),
]

# I am so lazy...
MHT = TypeVar("MHT", bound="MarkdownHeader")


class TestMarkdownHeader:
    @pytest.mark.parametrize("line", HEADER_POSITIVE_CASES.keys())
    def test_is_header_positive(self, line: str):
        assert MarkdownHeader.is_header(line) is True

    @pytest.mark.parametrize("line", HEADER_NEGATIVE_CASES)
    def test_is_header_negative(self, line: str):
        assert MarkdownHeader.is_header(line) is False

    @pytest.mark.parametrize(
        "line, level, title", [(key, value[0], value[1]) for key, value in HEADER_POSITIVE_CASES.items()]
    )
    def test_from_line(self, line: str, level: int, title: str):
        expected = MarkdownHeader(level, title)
        actual = MarkdownHeader.from_line(line)
        assert actual.level == expected.level
        assert actual.title == expected.title

    @pytest.mark.parametrize(
        "parent, child", [(MarkdownHeader.from_line("# Duvet Specification"), MarkdownHeader.from_line("## Overview"))]
    )
    def test_add_child_positive(self, parent: MarkdownHeader, child: MarkdownHeader):
        parent.add_child(child)
        assert len(parent.childHeaders) == 1
        assert parent.childHeaders[0] == child
        assert child.parentHeader == parent

    @pytest.mark.parametrize(
        "parent, child", [(MarkdownHeader.from_line("## Overview"), MarkdownHeader.from_line("# Duvet Specification"))]
    )
    def test_add_child_negative(self, parent: MarkdownHeader, child: MarkdownHeader):
        with pytest.raises(AssertionError):
            parent.add_child(child)

    @pytest.mark.parametrize(
        "parent, child, sibling",
        [
            (
                MarkdownHeader.from_line("# Duvet Specification"),
                MarkdownHeader.from_line("## Overview"),
                MarkdownHeader.from_line("## Editing"),
            )
        ],
    )
    def test_add_sibling_positive(self, parent: MHT, child: MHT, sibling: MHT):
        parent.childHeaders = []  # There is a weird thing where pytest is reloading parent from earlier
        parent.add_child(child)
        child.add_sibling(sibling)
        print([child.title for child in parent.childHeaders])
        assert len(parent.childHeaders) == 2
        assert child.parentHeader == parent
        assert sibling.parentHeader == parent

    @pytest.mark.parametrize(
        "parent, child, sibling",
        [
            (
                MarkdownHeader.from_line("# Duvet Specification"),
                MarkdownHeader.from_line("## Overview"),
                MarkdownHeader.from_line("# Editing"),
            )
        ],
    )
    def test_add_sibling_negative(self, parent: MHT, child: MHT, sibling: MHT):
        parent.add_child(child)
        with pytest.raises(AssertionError):
            child.add_sibling(sibling)

    @pytest.mark.parametrize(
        "parent, child, expected",
        [
            (
                MarkdownHeader.from_line("# Parent Title"),
                MarkdownHeader.from_line("## Odd.Name.But.We.Will.Allow.It"),
                "Parent-Title.Odd_Name_But_We_Will_Allow_It",
            )
        ],
    )
    def test_get_url(self, parent: MarkdownHeader, child: MarkdownHeader, expected: str):
        parent.add_child(child)
        assert child.get_url() == expected

    # TODO: test_from_match


class TestMarkdownSpecification:
    @pytest.mark.parametrize("filename", ["markdown.md", "another/markdown.md"])
    def test_is_markdown_positive(self, filename):
        assert MarkdownSpecification.is_markdown(filename) is True

    @pytest.mark.parametrize("filename", ["not_markdown.rts", "another/markdown.txt"])
    def test_is_markdown_negative(self, filename):
        assert MarkdownSpecification.is_markdown(filename) is False
