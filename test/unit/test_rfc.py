# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Unit test suite for duvet.markdown."""
import pathlib
from typing import Callable, List

import pytest

from duvet.rfc import RFCHeader, RFCSpecification
from duvet.specification_parser import MAX_HEADER_LEVELS, Span

from ..utils import populate_file  # isort:skip

pytestmark = [pytest.mark.unit, pytest.mark.local]

HEADER_POSITIVE_CASES = {
    "1.  Duvet Specification": (1, "Duvet Specification", "1."),
    "1.2.  Overview !@#$%^&*()_+": (2, "Overview !@#$%^&*()_+", "1.2."),
}
HEADER_NEGATIVE_CASES = [
    "#",
    "#  ",
    "#\n",
    "#\t",
    "#\r",
    "#\f",
    "#\v",
    "".join(["#" for i in range(0, int(MAX_HEADER_LEVELS))]),
]


class TestRFCHeader:
    @staticmethod
    @pytest.mark.parametrize(
        "line, is_header",
        [(key, True) for key in HEADER_POSITIVE_CASES.keys()]  # pylint: disable=C0201
        + [(mem, False) for mem in HEADER_NEGATIVE_CASES],
    )
    def test_is_header(line: str, is_header: bool):
        assert RFCHeader.is_header(line) is is_header

    @staticmethod
    @pytest.mark.parametrize(
        "line, level, title, number",
        [(key, value[0], value[1], value[2]) for key, value in HEADER_POSITIVE_CASES.items()],
    )
    def test_from_line(line: str, level: int, title: str, number: str):
        expected = RFCHeader(level, title, number)
        actual: RFCHeader = RFCHeader.from_line(line)
        assert actual.level == expected.level
        assert actual.title == expected.title
        assert actual.number == expected.number

    @staticmethod
    @pytest.mark.parametrize(
        "parent, child", [(RFCHeader.from_line("2.1.  Introduction"), RFCHeader.from_line("2.1.1.  Overview"))]
    )
    def test_add_child_positive(parent: RFCHeader, child: RFCHeader):
        parent.add_child(child)
        assert len(parent.children) == 1
        assert parent.children[0] == child
        assert child.parent == parent

    @staticmethod
    @pytest.mark.parametrize(
        "parent, child", [(RFCHeader.from_line("2.1.1.  Overview"), RFCHeader.from_line("2.1.  Duvet Specification"))]
    )
    def test_add_child_negative(parent: RFCHeader, child: RFCHeader):
        with pytest.raises(AssertionError):
            parent.add_child(child)

    @staticmethod
    @pytest.mark.parametrize(
        "parent, child, expected",
        [
            (
                RFCHeader.from_line("2.1.  Parent Title"),
                RFCHeader.from_line("2.1.1.  Odd.Name.But.We.Will.Allow.It"),
                "Parent-Title.Odd_Name_But_We_Will_Allow_It",
            )
        ],
    )
    def test_get_url(parent: RFCHeader, child: RFCHeader, expected: str):
        parent.add_child(child)
        assert child.get_url() == expected


class TestRFCSpecification:
    @staticmethod
    @pytest.mark.parametrize(
        "filename, is_rfc",
        [(filename, True) for filename in ["rfc.txt", "another/rfc.txt"]]
        + [(filename, False) for filename in ["not_rfc.rts", "another/markdown.md"]],
    )
    def test_is_rfc(filename: str, is_rfc: bool):
        assert RFCSpecification.is_file_format(filename) is is_rfc

    @staticmethod
    def test_simple(tmp_path):
        expected_top_title = "1.  Main Title"
        expected_top_body = "\nBody"
        expected_content = expected_top_title + expected_top_body
        expected_title = "rfc.txt"
        expected_top = RFCHeader.from_line(expected_top_title)
        expected_top.set_body(Span(len(expected_top_title), len(expected_content)))
        expected_top.title_span = Span(0, len(expected_top_title))
        filepath = populate_file(tmp_path, expected_content, expected_title)
        actual = RFCSpecification.parse(filepath=filepath)
        assert actual.filepath == filepath
        assert actual.title == expected_title
        # Tests that Spec reads file
        assert actual.content == expected_content
        # Tests that Spec finds top header
        assert len(actual.children) == 1
        assert actual.children[0].title == expected_top.title
        actual_top = actual.children[0]
        # Tests that spec set top header correctly
        assert actual_top.title_span == expected_top.title_span
        assert actual_top.body_span == expected_top.body_span
        # assert actual_top.specfication == actual
        assert actual_top.root == actual
        assert actual_top.get_body() == expected_top_body
        assert actual_top.validate() is True

    @staticmethod
    def execute(filepath: pathlib.Path, markdown_block: str, get_expected_top: Callable[[], List[RFCHeader]]):
        actual: RFCHeader = RFCSpecification.parse(filepath=populate_file(filepath, markdown_block, "rfc.txt"))
        expected_top: List[RFCHeader] = get_expected_top()
        # Verify that the tree is correct by checking against the expected titles
        assert [hdr.title for hdr in actual.children] == [hdr.title for hdr in expected_top]
        assert [hdr.title for hdr in actual.children[0].descendants] == [
            hdr.title for hdr in expected_top[0].descendants
        ]
        # Verify that all Headers in the tree are complete
        assert all(hdr.validate() for hdr in actual.descendants)

    @staticmethod
    def test_header_tree_assembly_happy(tmp_path):
        markdown_block = (
            "\n1.  Main Title\n\n"
            "1.1.  A Section\n\n"
            "1.1.1.  A Sub Section\n\n"
            "1.2.  Another Section\n\n"
            "1.3.  Another Another Section\n\n"
            "2.  Another Title\n"
        )

        def get_expected_top() -> List[RFCHeader]:
            top: RFCHeader = RFCHeader.from_line("1.  Main Title")
            top.add_child(RFCHeader.from_line("1.1.  A Section"))
            top.add_child(RFCHeader.from_line("1.2.  Another Section"))
            top.children[0].add_child(RFCHeader.from_line("1.1.1.  A Sub Section"))
            top.add_child(RFCHeader.from_line("1.3.  Another Another Section"))
            another_top: RFCHeader = RFCHeader.from_line("2.  Another Title")
            return [top, another_top]

        TestRFCSpecification.execute(tmp_path, markdown_block, get_expected_top)

    @staticmethod
    def test_header_tree_assembly_skip(tmp_path):
        markdown_block = "\n1.  Main Title\n\n1.1.1.  A Sub Section\n\n2.1.1.  Another Sub Section\n\n2.2.  A Section\n"

        def get_expected_top() -> List[RFCHeader]:
            top: RFCHeader = RFCHeader.from_line("1.  Main Title")
            top.add_child(RFCHeader.from_line("1.1.1.  A Sub Section"))
            top.add_child(RFCHeader.from_line("2.1.1.  Another Sub Section"))
            top.add_child(RFCHeader.from_line("2.2.  A Section"))
            return [top]

        TestRFCSpecification.execute(tmp_path, markdown_block, get_expected_top)

    @staticmethod
    def test_header_tree_assembly_start_not_one(tmp_path):
        markdown_block = "\n1.1.  A Section\n\n1.1.1.  A Sub Section\n\n2.1.  Another Section\n\n3.  A Title"

        def get_expected_top() -> List[RFCHeader]:
            rtn: list = [RFCHeader.from_line("1.1.  A Section")]
            rtn[0].add_child(RFCHeader.from_line("1.1.1.  A Sub Section"))
            rtn.append(RFCHeader.from_line("2.1.  Another Section"))
            rtn.append(RFCHeader.from_line("3.  A Title"))
            return rtn

        TestRFCSpecification.execute(tmp_path, markdown_block, get_expected_top)

    @staticmethod
    def test_header_tree_assembly_jump_back(tmp_path):
        markdown_block = "\n1.  Main Title\n\n1.1.  A Section\n\n1.1.1.  A Sub Section\n\n2.  Another Title\n"

        def get_expected_top() -> List[RFCHeader]:
            top: RFCHeader = RFCHeader.from_line("1.  Main Title")
            top.add_child(RFCHeader.from_line("1.1.  A Section"))
            top.children[0].add_child(RFCHeader.from_line("1.1.1.  A Sub Section"))
            another_top: RFCHeader = RFCHeader.from_line("2.  Another Title")
            return [top, another_top]

        TestRFCSpecification.execute(tmp_path, markdown_block, get_expected_top)
