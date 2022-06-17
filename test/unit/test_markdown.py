# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Unit test suite for duvet.markdown."""
from typing import List

import pytest

from duvet.markdown import MAX_HEADER_LEVELS, MarkdownHeader, MarkdownSpecification, Span

from ..utils import populate_file  # isort:skip

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
        assert len(parent.children) == 1
        assert parent.children[0] == child
        assert child.parent == parent

    @pytest.mark.parametrize(
        "parent, child", [(MarkdownHeader.from_line("## Overview"), MarkdownHeader.from_line("# Duvet Specification"))]
    )
    def test_add_child_negative(self, parent: MarkdownHeader, child: MarkdownHeader):
        with pytest.raises(AssertionError):
            parent.add_child(child)

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

    @staticmethod
    def test_simple(tmp_path):
        expected_content = "# Main Title\nBody"
        expected_title = "markdown.md"
        expected_top = MarkdownHeader.from_line("# Main Title")
        expected_top.set_body(Span(12, len(expected_content)))
        expected_top.title_span = Span(0, 12)
        filepath = populate_file(tmp_path, expected_content, expected_title)
        actual = MarkdownSpecification(filepath=filepath)
        assert actual.filepath == filepath
        assert actual.title == expected_title
        # Tests that Spec reads file
        assert actual.content == expected_content
        # Tests that Spec finds top header
        assert actual.cursor.title == expected_top.title
        assert len(actual.headers) == 1
        assert actual.headers[0].title == expected_top.title
        actual_top = actual.headers[0]
        # Tests that spec sets top header span's correctly
        assert actual_top.title_span == expected_top.title_span
        assert actual_top.body_span == expected_top.body_span
        assert actual_top.specification == actual
        assert actual_top.get_body() == "\nBody"
        assert actual_top.validate() is True

    @staticmethod
    def test_header_tree_assembly_happy(tmp_path):
        markdown_block = (
            "\n# Main Title\n\n"
            "## A Section\n\n"
            "### A Sub Section\n\n"
            "## Another Section\n\n"
            "## Another Another Section\n\n"
            "# Another Title\n"
        )

        def get_expected_top() -> List[MarkdownHeader]:
            top = MarkdownHeader.from_line("# Main Title")
            top.add_child(MarkdownHeader.from_line("## A Section"))
            top.add_child(MarkdownHeader.from_line("## Another Section"))
            top.children[0].add_child(MarkdownHeader.from_line("### A Sub Section"))
            top.add_child(MarkdownHeader.from_line("## Another Another Section"))
            another_top = MarkdownHeader.from_line("# Another Title")
            return [top, another_top]

        actual = MarkdownSpecification(filepath=populate_file(tmp_path, markdown_block, "markdown.md"))
        expected_top = get_expected_top()
        # Verify that the tree is correct by checking against the expected headers titles
        assert [hdr.title for hdr in actual.headers] == [hdr.title for hdr in expected_top]
        assert [hdr.title for hdr in actual.headers[0].children] == [hdr.title for hdr in expected_top[0].children]
        assert [hdr.title for hdr in actual.headers[0].children[0].children] == [
            hdr.title for hdr in expected_top[0].children[0].children
        ]
        # Verify that all Headers in the tree are complete
        assert all([hdr.validate() for hdr in actual.headers])
        assert all([hdr.validate() for hdr in actual.headers[0].children])
        assert all([hdr.validate() for hdr in actual.headers[0].children[0].children])

    @staticmethod
    def test_header_tree_assembly_skip(tmp_path):
        markdown_block = "\n# Main Title\n\n" "### A Sub Section\n\n" "### Another Sub Section\n\n" "## A Section\n"

        def get_expected_top() -> List[MarkdownHeader]:
            top = MarkdownHeader.from_line("# Main Title")
            top.add_child(MarkdownHeader.from_line("### A Sub Section"))
            top.add_child(MarkdownHeader.from_line("### Another Sub Section"))
            top.add_child(MarkdownHeader.from_line("## A Section"))
            return [top]

        actual = MarkdownSpecification(filepath=populate_file(tmp_path, markdown_block, "markdown.md"))
        expected_top = get_expected_top()
        # Verify that the tree is correct by checking against the expected headers titles
        assert [hdr.title for hdr in actual.headers] == [hdr.title for hdr in expected_top]
        assert [hdr.title for hdr in actual.headers[0].children] == [hdr.title for hdr in expected_top[0].children]
        assert [hdr.title for hdr in actual.headers[0].children[0].children] == [
            hdr.title for hdr in expected_top[0].children[0].children
        ]

    @staticmethod
    def test_header_tree_assembly_start_not_one(tmp_path):
        markdown_block = "\n## A Section\n\n" "### A Sub Section\n\n" "## Another Section\n\n" "# A Title"

        def get_expected_top() -> List[MarkdownHeader]:
            rtn = [MarkdownHeader.from_line("## A Section")]
            rtn[0].add_child(MarkdownHeader.from_line("### A Sub Section"))
            rtn.append(MarkdownHeader.from_line("## Another Section"))
            rtn.append(MarkdownHeader.from_line("# A Title"))
            return rtn

        actual = MarkdownSpecification(filepath=populate_file(tmp_path, markdown_block, "markdown.md"))
        expected_top = get_expected_top()
        # Verify that the tree is correct by checking against the expected headers titles
        assert [hdr.title for hdr in actual.headers] == [hdr.title for hdr in expected_top]
        assert [hdr.title for hdr in actual.headers[0].children] == [hdr.title for hdr in expected_top[0].children]
        assert [hdr.title for hdr in actual.headers[0].children[0].children] == [
            hdr.title for hdr in expected_top[0].children[0].children
        ]

    @staticmethod
    def test_header_tree_assembly_jump_back(tmp_path):
        markdown_block = "\n" "# Main Title\n" "\n" "## A Section\n" "\n" "### A Sub Section\n" "\n" "# Another Title\n"

        def get_expected_top() -> List[MarkdownHeader]:
            top = MarkdownHeader.from_line("# Main Title")
            top.add_child(MarkdownHeader.from_line("## A Section"))
            top.children[0].add_child(MarkdownHeader.from_line("### A Sub Section"))
            another_top = MarkdownHeader.from_line("# Another Title")
            return [top, another_top]

        actual = MarkdownSpecification(filepath=populate_file(tmp_path, markdown_block, "markdown.md"))
        expected_top = get_expected_top()
        # Verify that the tree is correct by checking against the expected headers titles
        assert [hdr.title for hdr in actual.headers] == [hdr.title for hdr in expected_top]
        assert [hdr.title for hdr in actual.headers[0].children] == [hdr.title for hdr in expected_top[0].children]
        assert [hdr.title for hdr in actual.headers[0].children[0].children] == [
            hdr.title for hdr in expected_top[0].children[0].children
        ]
