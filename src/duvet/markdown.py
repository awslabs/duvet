# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Methods and classes for parsing Markdown files."""
import os
import re
from typing import List, TypeVar, Optional, Iterable

from attr import define, field

MAX_HEADER_LEVELS: int = 4
# From start of string                                 :: ^
# Match at least 1 up to MAX_HEADER_LEVELS "#"         :: #{1,MAX_HEADER_LEVELS}
# followed by 1 or more white space excluding new line :: [ \t\r\f\v]+
# followed by 1 or more not white space                :: [^\s]+
# followed by 0 or more not newline                    :: [^\n]*
HEADER_REGEX = r"(^#{1," + str(MAX_HEADER_LEVELS) + r"}[ \t\r\f\v]+[^\s]+[^\n]*)"
# Match A Markdown Header
IS_HEADER_REGEX = re.compile(HEADER_REGEX)
# Match All Markdown Headers
ALL_HEADERS_REGEX = re.compile(HEADER_REGEX, re.MULTILINE)

MarkdownHeaderT = TypeVar("MarkdownHeaderT", bound="MarkdownHeader")
MarkdownSpecT = TypeVar("MarkdownSpecT", bound="MarkdownSpecification")


@define
class MarkdownHeader:
    """Represents a Markdown Header.

    Facilitates creating a Header Tree."""
    level: int = field(init=True)
    title: str = field(init=True)
    body: Optional[str] = field(init=False, default=None)
    childHeaders: List[MarkdownHeaderT] = field(init=False, default=[])
    parentHeader: Optional[MarkdownHeaderT] = field(init=False, default=None)

    @staticmethod
    def is_header(line: str) -> bool:
        """Detect Markdown header."""
        return True if IS_HEADER_REGEX.fullmatch(line) else False

    @staticmethod
    def from_line(line: str) -> MarkdownHeaderT:
        """Generate a Markdown Header from a line."""
        assert MarkdownHeader.is_header(line)
        hashes, title = line.split(maxsplit=1)
        return MarkdownHeader(level=len(hashes), title=title.strip())

    @staticmethod
    def from_match(match: re.Match, end_body: Optional[int]) -> MarkdownHeaderT:
        """Generate a Markdown Header from a re.Match."""
        cls = MarkdownHeader.from_line(match.string[match.start():match.end()])
        cls.body = match.string[match.end():end_body]
        return cls

    def add_child(self, child: MarkdownHeaderT):
        """Adds a child Markdown Header."""
        assert self.level < child.level
        child.set_parent(self)
        self.childHeaders.append(child)

    def set_parent(self, parent: MarkdownHeaderT):
        """Sets the parent Markdown Header"""
        assert self.level > parent.level
        self.parentHeader = parent

    def get_url(self) -> str:
        url: str = self.title.replace(' ', '-').replace('.', '_')
        header_cursor: MarkdownHeader = self.parentHeader
        while header_cursor is not None:
            cursor_url = header_cursor.title.replace(' ', '-').replace('.', '_')
            url = ".".join([cursor_url, url])
            header_cursor = header_cursor.parentHeader
        return url


@define
class MarkdownSpecification:
    filepath: os.PathLike = field(init=True)
    title: str = field(init=False)
    char_cursor: int = field(init=False, default=0)
    match_cursor: Optional[re.Match] = field(init=False, default=None)
    match_iter: Optional[Iterable[re.Match]] = field(init=False, default=None)
    top_headers: List[MarkdownHeader] = field(init=False, default=[])

    @staticmethod
    def is_markdown(filename: str) -> bool:
        return filename.rsplit(".", 1)[-1] in ["md"]


    # Parsing Logic:
    # -- Use Regex to find all headers
    # -- Create Header Tree
