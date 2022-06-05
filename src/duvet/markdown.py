# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Methods and classes for parsing Markdown files."""
import re
from pathlib import Path
from typing import List, Optional, TypeVar

import attr
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
SpanT = TypeVar("SpanT", bound="Span")


@define
class Span:
    """The start and end indexes of sub-string in a block."""

    start: int = field(init=True)
    end: int = field(init=True)

    def __attrs_post_init__(self):
        """Validate that start is before end."""
        assert self.start < self.end, f"Start must be less than end. {self.start} !< {self.end}"

    @classmethod
    def from_match(cls, match: re.Match):
        """Span from re.Match."""
        start, end = match.span()
        # noinspection PyArgumentList
        return cls(start, end)


@define
class MarkdownHeader:
    """Represent a Markdown Header.

    Facilitates creating a Header Tree.
    """

    level: int = field(init=True, repr=False)
    title: str = field(init=True, repr=True)
    child_headers: List[MarkdownHeaderT] = field(init=False, default=attr.Factory(list), repr=True)
    parent_header: Optional[MarkdownHeaderT] = field(init=False, default=None, repr=False)
    title_span: Span = field(init=False, default=None, repr=False)
    body_span: Span = field(init=False, default=None, repr=False)
    specification: MarkdownSpecT = field(init=False, default=None, repr=False)

    @staticmethod
    def is_header(line: str) -> bool:
        """Detect Markdown header."""
        return bool(IS_HEADER_REGEX.fullmatch(line))

    @staticmethod
    def from_line(line: str) -> MarkdownHeaderT:
        """Generate a Markdown Header from a line."""
        assert MarkdownHeader.is_header(line), f"line: {line} is not a Markdown header."
        hashes, title = line.split(maxsplit=1)
        return MarkdownHeader(level=len(hashes), title=title.strip())

    @staticmethod
    def from_match(match: re.Match, spec: MarkdownSpecT) -> MarkdownHeaderT:
        """Generate a Markdown Header from a re.Match."""
        cls = MarkdownHeader.from_line(match.string[match.start() : match.end()])
        cls.title_span = Span.from_match(match)
        cls.specification = spec
        return cls

    def set_body(self, span: Span):
        """Set the body span."""
        self.body_span = span

    def get_body(self) -> str:
        """Get the body of the header."""
        assert self.specification is not None, "Cannot call get_body without a specification set"
        return self.specification.content[self.body_span.start : self.body_span.end]

    def add_child(self, child: MarkdownHeaderT):
        """Add a child Markdown Header."""
        assert self.level < child.level, f"Child's level: {child.level} is higher than parent's: {self.level}"
        assert child.child_headers == [], "Cannot add child that has children"
        child.set_parent(self)
        self.child_headers.append(child)

    def set_parent(self, parent: MarkdownHeaderT):
        """Set the parent Markdown Header."""
        assert self.level > parent.level, f"Child's level: {self.level} is lower than parent's: {parent.level}"
        self.parent_header = parent

    def add_sibling(self, sibling: MarkdownHeaderT):
        """Add a sibling Markdown Header."""
        assert (
            self.level == sibling.level
        ), f"Sibling's MUST have an equal level. self's level: {self.level}, sibling's level: {sibling.level}"
        assert self.parent_header is not None, "Parent-less headers cannot have siblings"
        self.parent_header.add_child(sibling)

    def get_url(self) -> str:
        """Prefixes parent headers titles to this.

        Titles are transformed as follows:
        - spaces are replaced with "-"
        - "." are replaced with "_"
        """
        url: str = self.title.replace(" ", "-").replace(".", "_")
        header_cursor: MarkdownHeader = self.parent_header
        while header_cursor is not None:
            cursor_url = header_cursor.title.replace(" ", "-").replace(".", "_")
            url = ".".join([cursor_url, url])
            header_cursor = header_cursor.parent_header
        return url

    def validate(self) -> bool:
        """Check that all needed fields are set and reasonable."""
        # fmt: off
        return self.body_span is not None \
            and self.title_span is not None \
            and self.specification is not None \
            and len(self.specification.content) >= self.body_span.end
        # fmt: on


@define
class MarkdownSpecification:
    r"""Represent a Markdown Specification.

    The following assumptions are made about the structure of the Markdown File:
    1. A Markdown File is not massive
    2. If there are any Headers, the first Header is a level 1 header ("#")
    3. If there are any Headers, they never skip levels (i.e: NOT: "# Title\n### Sub Section\n")
    4. If there are any Headers, they never jump back
      (i.e: NOT: "# Title\n## Section\n### Sub Section\n# Another Title")
    5. The Markdown File is encoded in utf-8

    About (1): This class reads and processes the whole file at once,
    with no buffering.
    If the file is truly massive, larger than available memory,
    this class will fail.
    (1 & 5) are acceptable.

    (2-4) are not.
    We must re-work this class and its partner MarkdownHeader so (2-4) are addressed.
    """

    filepath: Path = field(init=True)
    title: str = field(init=False)
    content: str = field(init=False, default=None)
    cursor: Optional[MarkdownHeader] = field(init=False, default=None)
    top_headers: List[MarkdownHeader] = field(init=False, default=attr.Factory(list))

    @staticmethod
    def is_markdown(filename: str) -> bool:
        """Detect Markdown file."""
        return filename.rsplit(".", 1)[-1] == "md"

    def __attrs_post_init__(self):
        """Read Markdown file and create Header tree."""
        assert MarkdownSpecification.is_markdown(self.filepath.suffix), f"{self.filepath} does not end in .md"
        self.title = self.filepath.name
        with open(file=self.filepath, mode="rt", encoding="utf-8") as spec:
            self.content = spec.read()
        self._process()

    def _process(self):
        match_iter = ALL_HEADERS_REGEX.finditer(self.content)
        for match in match_iter:
            new_header = MarkdownHeader.from_match(match, self)
            self._insert_header(new_header)
            self._set_cursor_body(match)
            self.cursor = new_header
        self._handle_last_header()
        self.reset_header_cursor()

    def _insert_header(self, new_header: MarkdownHeader):
        """Insert a Header into the Markdown Tree.

        This method ASSUMES text is processed serially.
        It does NOT support arbitrary header insertion.
        """
        if self.cursor is None and new_header.level != 1:
            raise ValueError("_insert_header does NOT support arbitrary header insertion")
        if self.cursor is None or new_header.level == 1:
            self.top_headers.append(new_header)
        elif self.cursor.level < new_header.level:
            self.cursor.add_child(new_header)
        elif self.cursor.level == new_header.level:
            # The level is not 1, so there will be a parent.
            self.cursor.add_sibling(new_header)
        elif self.cursor.level > new_header.level:
            # Think of it as the cursor is on level 3,
            # and the new level is 2,
            # so 2 needs to be added to 1.
            # Thus, we add to the parent a sibling.
            self.cursor.parent_header.add_sibling(new_header)
        else:
            raise Exception("The logic for MarkdownSpecification._insert_header is incorrect.")

    def _set_cursor_body(self, match: re.Match):
        """Set the current headers body span."""
        if self.cursor:
            # From the end of title to the start of the next title.
            span = Span(self.cursor.title_span.end, match.start())
            self.cursor.set_body(span)

    def _handle_last_header(self):
        """Set the current headers body span."""
        if self.cursor:
            # From the end of title to the end of the file.
            span = Span(self.cursor.title_span.end, len(self.content))
            self.cursor.set_body(span)

    def reset_header_cursor(self):
        """Reset the header cursor to the first top header."""
        if len(self.top_headers) > 0:
            self.cursor = self.top_headers[0]  # pylint: disable=E1136
