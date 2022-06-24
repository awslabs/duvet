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
DEFAULT_ANNO_TYPE = AnnotationType.CITATION


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
        anno_blocks: list[LineSpan] = []
        start_anno: Optional[int] = None

        for index, line in enumerate(lines):
            anno_hit: Optional[re.Match] = self.is_anno.search(line)
            if anno_hit is None and start_anno is not None:
                anno_blocks.append(LineSpan(start=start_anno, end=index))
                start_anno = None
            elif anno_hit is not None and start_anno is None:
                start_anno = index
        # Edge case for annotation blocks that end the file
        if start_anno is not None:
            anno_blocks.append(LineSpan(start=start_anno, end=len(lines)))

        return anno_blocks

    def _extract_anno_kwargs(self, lines: list[str], anno_blocks: list[LineSpan]) -> list[dict]:
        """Parse none or more Annotation key word args from lines via LineSpans."""
        kwargs: list[dict] = []
        for anno_block in anno_blocks:
            index: int = anno_block.start
            while index < anno_block.end:
                start: int = index
                # fmt: off

                # the first line will be the url
                url: Optional[str] = (
                    self.match_url.match(lines[index]).__getitem__(1)
                    if self.match_url.match(lines[index])
                    else None
                )
                index += 1 if url is not None else 0

                # there may be a type
                _type: Optional[str] = (
                    self.match_type.match(lines[index]).__getitem__(1)
                    if self.match_type.match(lines[index])
                    else None
                )
                index += 1 if _type is not None else 0

                # there may be a reason;
                reason: Optional[str] = (
                    self.match_reason.match(lines[index]).__getitem__(1)
                    if self.match_reason.match(lines[index])
                    else None
                )
                index += 1 if reason is not None else 0

                # there MUST be content
                content = ""
                while index < len(lines) and self.match_content.match(lines[index]):
                    content += self.match_content.match(lines[index]).__getitem__(1) + "\n"
                    index += 1

                kwarg = {"target": url, "type": _type, "start": start,
                         "end": index, "reason": reason, "content": content}
                kwargs.append(kwarg)
                # assert url is not None, f"url is None on anno start {start}"
                # assert content is not None, f"content is None on anno start {start}"
                # fmt: on

        return kwargs

    def _process_anno_kwargs(self, anno_kwargs: list[dict], filepath: Path) -> list[Annotation]:
        pass

    def process_file(self, filepath: Path) -> list[dict]:
        """Extract annotations from one file."""

        with open(filepath, "r", encoding="utf-8") as implementation_file:
            lines: list[str] = implementation_file.readlines()

        anno_blocks: list[LineSpan] = self._extract_blocks(lines)
        anno_kwargs: list[dict] = self._extract_anno_kwargs(lines, anno_blocks)
        return anno_kwargs
#//