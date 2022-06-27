# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Annotation Parser used by duvet-python."""
import pytest

from duvet._config import DEFAULT_CONTENT_STYLE, DEFAULT_META_STYLE
from duvet.annotation_parser import AnnotationParser, LineSpan

pytestmark = [pytest.mark.unit, pytest.mark.local]


@pytest.fixture
def under_test(tmp_path) -> AnnotationParser:
    return AnnotationParser([tmp_path])


# pylint: disable=R0201
class TestExtractSpans:
    @pytest.mark.parametrize(
        "text, expected_spans",
        [
            (f"\n{DEFAULT_META_STYLE} content\n", [LineSpan(1, 2)]),
            (f"\n{DEFAULT_CONTENT_STYLE} content\n", [LineSpan(1, 2)]),
            (f"{DEFAULT_CONTENT_STYLE} content\n", [LineSpan(0, 1)]),
            (f"\n{DEFAULT_CONTENT_STYLE} content", [LineSpan(1, 2)]),
            ("\ncontent\n", []),
            (f"\n{DEFAULT_META_STYLE} content\n{DEFAULT_CONTENT_STYLE} content\n", [LineSpan(1, 3)]),
            (f"\n{DEFAULT_META_STYLE} content\n\n{DEFAULT_CONTENT_STYLE} content\n", [LineSpan(1, 2), LineSpan(3, 4)]),
        ],
    )

    def test_extract(self, under_test, text: str, expected_spans: list[LineSpan]):
        lines = text.splitlines(keepends=True)
        actual_spans = under_test._extract_blocks(lines)
        assert expected_spans == actual_spans

# pylint: disable=R0201
class TestExtractkwargs:
    test_str = "//= target\n//= type=implication\n//# Duvet MUST\n"
    nested_str = "some code\n//= target\n//= type=implication\n//# Duvet MUST\nsome code\n"

    @pytest.mark.parametrize(
        "lines, spans, expected_dicts",
        [
            (
                    test_str.splitlines(keepends=True),
                    [LineSpan(0, 3)],
                    [
                        {
                            "content": "Duvet MUST",
                            "end_line": 3,
                            "reason": None,
                            "start_line": 0,
                            "target": "target",
                            "type": "implication",
                        }
                    ],
            ),
            (
                    nested_str.splitlines(True),
                    [LineSpan(1, 4)],
                    [
                        {
                            "content": "Duvet MUST",
                            "end_line": 4,
                            "reason": None,
                            "start_line": 1,
                            "target": "target",
                            "type": "implication",
                        }
                    ],
            ),
            (
                    (test_str + test_str).splitlines(True),
                    [LineSpan(0, 3), LineSpan(3, 6)],
                    [
                        {
                            "content": "Duvet MUST",
                            "end_line": 3,
                            "reason": None,
                            "start_line": 0,
                            "target": "target",
                            "type": "implication",
                        },
                        {
                            "content": "Duvet MUST",
                            "end_line": 6,
                            "reason": None,
                            "start_line": 3,
                            "target": "target",
                            "type": "implication",
                        },
                    ],
            ),
        ],
    )
    # pylint disable=no-self-use
    def test_extract(self, under_test, lines: list[str], spans: list[LineSpan], expected_dicts: list[dict]):
        actual_spans = under_test._extract_anno_kwargs(lines, spans)
        assert expected_dicts == actual_spans
