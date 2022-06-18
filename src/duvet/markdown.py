# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Methods and classes for parsing Markdown files."""
import re
from pathlib import Path
from typing import TypeVar, Union

from anytree import NodeMixin
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

SpanT = TypeVar("SpanT", bound="Span")
MarkdownElementT = TypeVar("MarkdownElementT", bound="MarkdownElement")
MarkdownHeaderT = TypeVar("MarkdownHeaderT", bound="MarkdownHeader")
MarkdownSpecT = TypeVar("MarkdownSpecT", bound="MarkdownSpecification")


@define
class Span:
    """The start and end indexes of sub-string in a block."""

    start: int = field(init=True)
    end: int = field(init=True)

    def __attrs_post_init__(self):
        """Validate that start is before end."""
        assert self.start <= self.end, f"Start must be less than end. {self.start} !< {self.end}"

    @staticmethod
    def from_match(match: re.Match) -> SpanT:
        """Span from re.Match."""
        start, end = match.span()
        return Span(start, end)


@define
class MarkdownElement(NodeMixin):
    """Either a Markdown file or header in a Markdown file."""

    level: int = field(init=True)
    title: str = field(init=True, repr=True)

    def add_child(self, child: MarkdownElementT):
        """Add a child Markdown Header."""
        assert self.level < child.level, f"Child's level: {child.level} is higher than parent's: {self.level}"
        assert len(child.children) == 0, "Cannot add child that has children"
        child.parent = self


@define
class MarkdownHeader(MarkdownElement):
    """Represent a Markdown Header.

    Facilitates creating a Header Tree.
    """

    title_span: Span = field(init=False, default=None, repr=False)
    body_span: Span = field(init=False, default=None, repr=False)

    @staticmethod
    def is_header(line: str) -> bool:
        """Detect Markdown header."""
        return bool(IS_HEADER_REGEX.fullmatch(line))

    @staticmethod
    def from_line(line: str) -> MarkdownHeaderT:
        """Generate a Markdown Header from a line."""
        assert MarkdownHeader.is_header(line), f"line: {line} is not a Markdown header."
        # str.split will split on whitespace, breaking hashes from everything else
        hashes, title = line.split(maxsplit=1)
        return MarkdownHeader(level=len(hashes), title=title.strip())

    @staticmethod
    def from_match(match: re.Match) -> MarkdownHeaderT:
        """Generate a Markdown Header from a re.Match."""
        cls = MarkdownHeader.from_line(match.string[match.start() : match.end()])
        cls.title_span = Span.from_match(match)
        return cls

    def set_body(self, span: Span):
        """Set the body span."""
        self.body_span = span

    def get_body(self) -> str:
        """Get the body of the header."""
        assert hasattr(self.root, "content"), "Cannot call get_body if self.root has no content attribute"
        assert isinstance(self.body_span, Span), "Cannot call get_body if self.body_span is not set"
        return self.root.content[self.body_span.start : self.body_span.end]

    def get_url(self) -> str:
        """Prefixes parent titles to this title.

        Titles are transformed as follows:
        - spaces are replaced with "-"
        - "." are replaced with "_"
        """
        url: str = self.title.replace(" ", "-").replace(".", "_")
        header_cursor: MarkdownHeader = self.parent
        while header_cursor is not None:
            cursor_url = header_cursor.title.replace(" ", "-").replace(".", "_")
            url = ".".join([cursor_url, url])
            header_cursor = header_cursor.parent
        return url

    def validate(self) -> bool:
        """Check that all needed fields are set and reasonable."""
        return (
            self.body_span is not None
            and self.title_span is not None
            and len(self.root.content) >= self.body_span.end
        )


@define
class MarkdownSpecification(MarkdownElement):
    """Represent a Markdown Specification.

    Creates a tree from the Markdown file's headers,
    with itself as the root of the tree.

    MarkdownSpecification extends anytree.NodeMixin,
    so all tree walking methods from anytree are supported.
    In particular, to view all the headers, use `descendants`.
    To check just the top level headers, use `children`.
    """

    filepath: Path = field(init=True, repr=False)
    cursor: Union[MarkdownHeader, MarkdownSpecT] = field(init=False)
    content: str = field(init=False, repr=False)

    @staticmethod
    def parse(filepath: Path) -> MarkdownSpecT:
        """Read Markdown file and create Header tree."""
        assert MarkdownSpecification.is_markdown(filepath.suffix), f"{filepath} does not end in .md"
        return MarkdownSpecification(filepath=filepath, title=filepath.name, level=0)

    @staticmethod
    def is_markdown(filename: str) -> bool:
        """Detect Markdown file."""
        return filename.rsplit(".", 1)[-1] == "md"

    def __attrs_post_init__(self):
        """Actually Read Markdown file and create Header tree."""
        self.cursor = self
        with open(file=self.filepath, mode="rt", encoding="utf-8") as spec:
            self.content = spec.read()
        self._process()

    def _process(self):
        match_iter = ALL_HEADERS_REGEX.finditer(self.content)
        for match in match_iter:
            new_header = MarkdownHeader.from_match(match)
            self._insert_header(self.cursor, new_header)
            self._set_cursor_body(match)
            self.cursor = new_header
        self._handle_last_header()
        self.reset_header_cursor()

    def _insert_header(self, cursor: MarkdownHeader, new_header: MarkdownHeader):
        """Insert a Header into the Markdown Tree.

        This method ASSUMES text is processed serially.
        """
        if cursor.level < new_header.level:
            for child in reversed(cursor.children):
                if child.level < new_header.level:
                    return child.add_child(new_header)
            cursor.add_child(new_header)
        elif cursor.level >= new_header.level:
            self._insert_header(cursor.parent, new_header)
        else:
            raise Exception("The logic for MarkdownSpecification._insert_header is incorrect.")

    def _set_cursor_body(self, match: re.Match):
        """Set the current cursor's body span."""
        if self.cursor != self:
            # From the end of title to the start of the next title.
            span = Span(self.cursor.title_span.end, match.start())
            self.cursor.set_body(span)

    def _handle_last_header(self):
        """Set the last header's body span."""
        if self.cursor != self:
            # From the end of title to the end of the file.
            span = Span(self.cursor.title_span.end, len(self.content))
            self.cursor.set_body(span)

    def reset_header_cursor(self):
        """Reset the cursor to root."""
        self.cursor = self
