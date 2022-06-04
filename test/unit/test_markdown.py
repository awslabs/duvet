# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Unit test suite for duvet.markdown."""
import pytest

from duvet.markdown import MarkdownHeader, MAX_HEADER_LEVELS

pytestmark = [pytest.mark.unit, pytest.mark.local]

HEADER_POSITIVE_CASES = {
    "# Duvet Specification": (1, "Duvet Specification"),
    "## Overview !@#$%^&*()_+": (2, "Overview !@#$%^&*()_+")
}
HEADER_NEGATIVE_CASES = [
    "#", "#  ", "#\n", "#\t", "#\r", "#\f", "#\v",
    "".join(["#" for i in range(0, MAX_HEADER_LEVELS)])
]


class TestMarkdownHeader:
    @pytest.mark.parametrize(
        "line", HEADER_POSITIVE_CASES.keys()
    )
    def test_is_header_positive(self, line: str):
        assert MarkdownHeader.is_header(line) is True

    @pytest.mark.parametrize(
        "line", HEADER_NEGATIVE_CASES
    )
    def test_is_header_negative(self, line: str):
        assert MarkdownHeader.is_header(line) is False

    @pytest.mark.parametrize(
        "line, level, title",
        [(key, value[0], value[1]) for key, value in HEADER_POSITIVE_CASES.items()]
    )
    def test_header_from_line(self, line: str, level: int, title: str):
        expected = MarkdownHeader(level, title, line)
        actual = MarkdownHeader.header_from_line(line)
        assert actual.level == expected.level
        assert actual.title == expected.title
        assert actual.content == expected.content
