# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Annotation Parser used by duvet-python."""
# pylint: disable=fixme
import logging
import re
from pathlib import Path
from typing import Optional

import attr
from attrs import define, field

from duvet._config import DEFAULT_CONTENT_STYLE, DEFAULT_META_STYLE
from duvet.identifiers import AnnotationType
from duvet.structures import Annotation

__all__ = ["AnnotationParser", "LineSpan"]
_LOGGER = logging.getLogger(__name__)
DEFAULT_ANNO_TYPE_NAME = AnnotationType.CITATION.name


@define
class LineSpan:
    """Represents a span of lines."""

    start: int = field(init=True)
    end: int = field(init=True)


@define
class AnnotationParser:
    """Parser for annotation from implementation."""

    paths: list[Path] = field(init=True, default=attr.Factory(list), repr=False)
    annotations: list[Annotation] = field(init=False, default=attr.Factory(list), repr=False)
    # TODO: Sanitize user input for regular expression usage;
    # //= compliance/duvet-specification.txt#2.3.1
    # //= type=implication
    # //# This identifier of meta parts MUST
    # //# be configurable.
    meta_style: str = field(init=True, default=DEFAULT_META_STYLE)
    content_style: str = field(init=True, default=DEFAULT_CONTENT_STYLE)

    is_anno: re.Pattern = field(init=False, repr=False)
    match_url: re.Pattern = field(init=False, repr=False)
    match_type: re.Pattern = field(init=False, repr=False)
    match_reason: re.Pattern = field(init=False, repr=False)
    match_content: re.Pattern = field(init=False, repr=False)

    def __attrs_post_init__(self):
        """Set regular expression attributes."""
        pattern: str = f"((?:{self.meta_style})|(?:{self.content_style}))"
        self.is_anno = re.compile(pattern)
        self.match_url = re.compile(r"[\s]*" + self.meta_style + r"[\s](.*?)\n")
        self.match_type = re.compile(r"[\s]*" + self.meta_style + r"[\s]type=(.*?)\n")
        self.match_reason = re.compile(r"[\s]*" + self.meta_style + r"[\s]reason=(.*?)\n")
        self.match_content = re.compile(r"[\s]*" + self.content_style + r"[\s]*(.*?)\n")

    def _extract_blocks(self, lines: list[str]) -> list[LineSpan]:
        """Extract Annotation blocks from a file."""
        anno_spans: list[LineSpan] = []
        start_anno: Optional[int] = None

        for index, line in enumerate(lines):
            anno_hit: Optional[re.Match] = self.is_anno.search(line)
            if anno_hit is None and start_anno is not None:
                anno_spans.append(LineSpan(start=start_anno, end=index))
                start_anno = None
            elif anno_hit is not None and start_anno is None:
                start_anno = index
        # Edge case for annotation blocks that end the file
        if start_anno is not None:
            anno_spans.append(LineSpan(start=start_anno, end=len(lines)-1))

        return anno_spans

    def _extract_anno_kwargs(self, lines: list[str], spans: list[LineSpan]) -> list[dict]:
        """Parse none or more Annotation key word args from lines via LineSpans."""
        kwargs: list[dict] = []
        for span in spans:
            index: int = span.start
            while index < span.end:
                start: int = index

                # the first line will be the url
                match = self.match_url.match(lines[index])
                url: Optional[str] = match.__getitem__(1) if isinstance(match, re.Match) else None
                index += 1 if url is not None else 0
                del match

                # there may be a type
                match = self.match_type.match(lines[index])
                _type: Optional[str] = match.__getitem__(1) if isinstance(match, re.Match) else None
                index += 1 if _type is not None else 0
                del match

                # there may be a reason;
                match = self.match_reason.match(lines[index])
                reason: Optional[str] = match.__getitem__(1) if isinstance(match, re.Match) else None
                index += 1 if reason is not None else 0
                del match

                # there MUST be content
                content = ""
                match = self.match_content.match(lines[index])
                while index < span.end and isinstance(match, re.Match):
                    content += match.__getitem__(1) + "\n"
                    index += 1
                    match = self.match_content.match(lines[index]) if index < span.end else None
                del match

                # fmt: off
                kwarg = {"target": url, "type": _type, "start_line": start,
                         "end_line": index, "reason": reason, "content": content}
                kwargs.append(kwarg)
                # fmt: on

        return kwargs

    @staticmethod
    def _process_anno_kwargs(anno_kwargs: list[dict], filepath: Path) -> list[Annotation]:
        """Convert anno kwargs to Annotations."""
        rtn: list[Annotation] = []
        for kwarg in anno_kwargs:
            if kwarg.get('content') == "" or kwarg.get('target') is None:
                continue
            kwarg['type'] = DEFAULT_ANNO_TYPE_NAME if kwarg['type'] is None else kwarg['type']
            try:
                kwarg['type'] = AnnotationType[kwarg['type'].upper()]
            except KeyError:
                _LOGGER.warning("%s: Unknown type: %s found in lines %s to %s. Skipping",
                                filepath, kwarg['type'], kwarg['start_line'], kwarg['end_line'])
                continue
            kwarg['location'] = str(filepath)
            kwarg['uri'] = "$".join([kwarg['target'], kwarg['content']])
            rtn.append(Annotation(**kwarg))
        return rtn

    def process_file(self, filepath: Path) -> list[Annotation]:
        """Extract annotations from one file."""

        with open(filepath, "r", encoding="utf-8") as implementation_file:
            lines: list[str] = implementation_file.readlines()

        spans: list[LineSpan] = self._extract_blocks(lines)
        anno_kwargs: list[dict] = self._extract_anno_kwargs(lines, spans)
        return self._process_anno_kwargs(anno_kwargs, filepath)
