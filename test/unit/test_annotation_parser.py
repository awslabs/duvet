# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Annotation Parser used by duvet-python."""
import logging
from copy import deepcopy

import pytest

from duvet._config import DEFAULT_CONTENT_STYLE, DEFAULT_META_STYLE
from duvet.annotation_parser import AnnotationParser, LineSpan

pytestmark = [pytest.mark.unit, pytest.mark.local]
VALID_ANNO_KWARGS = {
    "target": "target",
    "type": "implication",
    "start_line": 0,
    "end_line": 3,
    "reason": None,
    "content": "Duvet MUST",
}
TEST_STR = f"{DEFAULT_META_STYLE} target\n{DEFAULT_META_STYLE} type=implication\n{DEFAULT_CONTENT_STYLE} Duvet MUST\n"


def _update_valid_kwargs(updates: dict) -> dict:
    rtn = deepcopy(VALID_ANNO_KWARGS)
    rtn.update(updates)
    return rtn


@pytest.fixture
def under_test(tmp_path) -> AnnotationParser:
    return AnnotationParser([tmp_path])


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
        actual_spans = under_test._extract_spans(lines)
        assert expected_spans == actual_spans


class TestExtractKwargs:
    nested_str = f"some code\n{TEST_STR}some code\n"

    @pytest.mark.parametrize(
        "lines, spans, expected_dicts",
        [
            (
                    TEST_STR.splitlines(True),
                    [LineSpan(0, 3)],
                    [deepcopy(VALID_ANNO_KWARGS)],
            ),
            (
                    nested_str.splitlines(True),
                    [LineSpan(1, 4)],
                    [_update_valid_kwargs({"start_line": 1, "end_line": 4})],
            ),
            (
                    (TEST_STR + TEST_STR).splitlines(True),
                    [LineSpan(0, 3), LineSpan(3, 6)],
                    [deepcopy(VALID_ANNO_KWARGS), _update_valid_kwargs({"start_line": 3, "end_line": 6})],
            ),
        ],
    )
    def test_extract(self, under_test, lines: list[str], spans: list[LineSpan], expected_dicts: list[dict]):
        actual_spans = under_test._extract_anno_kwargs(lines, spans)
        assert expected_dicts == actual_spans


class TestProcessKwargs:
    @pytest.mark.parametrize("kwarg", (_update_valid_kwargs({"target": None}), _update_valid_kwargs({"content": ""})))
    def test_skips_no_content_or_no_target(self, under_test, kwarg):
        actual_result = under_test._process_anno_kwargs([kwarg], under_test.paths[0])
        assert len(actual_result) == 0

    def test_skip_and_warns_unknown_type(self, under_test, caplog):
        kwarg = _update_valid_kwargs({"type": "Anton"})
        with caplog.at_level(logging.WARN):
            actual = under_test._process_anno_kwargs([kwarg], under_test.paths[0])
            assert len(actual) == 0
        assert len(caplog.messages) == 1
        assert "Unknown type: Anton found in lines 0 to 3. Skipping" in caplog.messages[0]

# //= compliance/duvet-specification.txt#2.3.4
# //= type=test
# //# It MUST start with "reason=".

# //= compliance/duvet-specification.txt#2.3.2
# //= type=test
# //# The first line of the meta part identifies the location of the content, it MUST be parsed as a URL.

# //= compliance/duvet-specification.txt#2.3.2
# //= type=test
# //# All parts of the URL other than a URL fragment MUST be optional and MUST identify
# //# the specification that contains this section and content.

# //= compliance/duvet-specification.txt#2.3.2
# //= type=test
# //# The URL MUST contain a URL fragment that uniquely identifies the section that contains this content.

# //= compliance/duvet-specification.txt#2.2.4.1
# //= type=test
# //# Duvet SHOULD be able to parse requirements formatted as Toml files.

# //= compliance/duvet-specification.txt#2.3.3
# //= type=test
# //# If the meta part is a single line then the type MUST be citation.

# //= compliance/duvet-specification.txt#2.3.3
# //= type=test
# //# If a second meta line exists it MUST start with "type=".

# //= compliance/duvet-specification.txt#2.3.3
# //= type=test
# //# The type MUST be a valid annotation type string:
