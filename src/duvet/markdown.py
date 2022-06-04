# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Methods and classes for parsing Markdown files."""
import os
import re
from typing import List, Optional, TypeVar

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
SpanT = TypeVar("SpanT", bound="_Span")


@define
class _Span:
    start: int = field(init=True)
    end: int = field(init=True)

    def __attrs_post_init__(self):
        assert self.start < self.end

    @classmethod
    def from_match(cls, match: re.Match):
        start, end = match.span()
        # noinspection PyArgumentList
        return cls(start, end)


@define
class MarkdownHeader:
    """Represent a Markdown Header.

    Facilitates creating a Header Tree.
    """

    level: int = field(init=True)
    title: str = field(init=True)
    childHeaders: List[MarkdownHeaderT] = field(init=False, default=[])
    parentHeader: Optional[MarkdownHeaderT] = field(init=False, default=None)
    title_span: _Span = field(init=False, default=None)
    body_span: _Span = field(init=False, default=None)
    specification: MarkdownSpecT = field(init=False, default=None)

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
    def from_match(match: re.Match, spec: MarkdownSpecT) -> MarkdownHeaderT:
        """Generate a Markdown Header from a re.Match."""
        cls = MarkdownHeader.from_line(match.string[match.start() : match.end()])
        cls.title_span = _Span.from_match(match)
        cls.specification = spec
        return cls

    def set_body(self, span: _Span):
        """Set the body span."""
        self.body_span = span

    def get_body(self) -> str:
        """Get the body of the header."""
        assert self.specification is not None
        return self.specification.content[self.body_span.start : self.body_span.end]

    def add_child(self, child: MarkdownHeaderT):
        """Add a child Markdown Header."""
        assert self.level < child.level
        child.set_parent(self)
        self.childHeaders.append(child)

    def set_parent(self, parent: MarkdownHeaderT):
        """Set the parent Markdown Header."""
        assert self.level > parent.level
        self.parentHeader = parent

    def add_sibling(self, sibling: MarkdownHeaderT):
        """Add a sibling Markdown Header."""
        assert self.level == sibling.level
        assert self.parentHeader is not None
        self.parentHeader.add_child(sibling)

    def get_url(self) -> str:
        """Prefixes parent headers titles to this.

        Titles are transformed as follows:
        - spaces are replaced with "-"
        - "." are replaced with "_"
        """
        url: str = self.title.replace(" ", "-").replace(".", "_")
        header_cursor: MarkdownHeader = self.parentHeader
        while header_cursor is not None:
            cursor_url = header_cursor.title.replace(" ", "-").replace(".", "_")
            url = ".".join([cursor_url, url])
            header_cursor = header_cursor.parentHeader
        return url

    def validate(self) -> bool:
        """Check that all needed fields are set."""
        # fmt: off
        return self.body_span is not None \
            and self.title_span is not None \
            and self.specification is not None
        # fmt: on


@define
class MarkdownSpecification:
    """Represent a Markdown Specification."""

    filepath: str = field(init=True)
    title: str = field(init=False)
    top_headers: List[MarkdownHeader] = field(init=False, default=[])
    content: str = field(init=False, default=[])
    header_cursor: Optional[MarkdownHeader] = field(init=False, default=None)

    @staticmethod
    def is_markdown(filename: str) -> bool:
        """Detect Markdown file."""
        return filename.rsplit(".", 1)[-1] == "md"

    def __attrs_post_init__(self):
        """Read Markdown file and create Header tree."""
        assert MarkdownSpecification.is_markdown(self.filepath)
        self.title = self.filepath.rsplit(os.sep, 1)[-1]
        content: str
        with open(file=self.filepath, mode="rt") as spec:
            self.content = spec.read()
        self._process()

    def _process(self):
        self.match_iter = ALL_HEADERS_REGEX.finditer(self.content)
        for match in self.match_iter:
            new_header = MarkdownHeader.from_match(match, self)
            self._insert_header(new_header)
            self._set_cursor_body(match)
            self.header_cursor = new_header
        self._handle_last_header()
        self.reset_header_cursor()

    def _insert_header(self, new_header: MarkdownHeader):
        """Insert a Header into the Markdown Tree.

        This method ASSUMES text is processed serially.
        It does NOT support arbitrary header insertion.
        """
        if self.header_cursor is None or new_header.level == 1:
            self.top_headers.append(new_header)
        elif self.header_cursor.level < new_header.level:
            self.header_cursor.add_child(new_header)
        elif self.header_cursor.level == new_header.level:
            # The level is not 1, so there will be a parent.
            self.header_cursor.add_sibling(new_header)
        elif self.header_cursor.level > new_header.level:
            # Think of it as the cursor is on level 3,
            # and the new level is 2,
            # so 2 needs to be added to 1.
            # Thus, we add to the parent a sibling.
            self.header_cursor.parentHeader.add_sibling(new_header)
        else:
            raise Exception("Impossible")

    def _set_cursor_body(self, match: re.Match):
        """Set the current headers body span."""
        if self.header_cursor:
            # From the end of title to the start of the next title.
            span = _Span(self.header_cursor.title_span.end, match.start())
            self.header_cursor.set_body(span)

    def _handle_last_header(self):
        """Set the current headers body span."""
        if self.header_cursor:
            # From the end of title to the end of the file.
            span = _Span(self.header_cursor.title_span.end, len(self.content))
            self.header_cursor.set_body(span)

    def reset_header_cursor(self):
        """Reset the header cursor to the first top header."""
        self.header_cursor = self.top_headers[0]
