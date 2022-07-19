# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Methods and classes for parsing Specification files."""
from abc import ABCMeta, abstractmethod  # isort: skip
from pathlib import Path
from re import Match
from typing import Iterator, TypeVar, Union

# We don't really need to check the type of third party library.
from anytree import NodeMixin # type: ignore[import]
from attrs import define, field

MAX_HEADER_LEVELS: str = str(4)
SpanT = TypeVar("SpanT", bound="Span")
SpecificationElementT = TypeVar("SpecificationElementT", bound="SpecificationElement")
SpecificationHeaderT = TypeVar("SpecificationHeaderT", bound="SpecificationHeader")
ParsedSpecificationT = TypeVar("ParsedSpecificationT", bound="ParsedSpecification")


@define
class Span:
    """The start and end indexes of sub-string in a block."""

    start: int = field(init=True)
    end: int = field(init=True)

    def __attrs_post_init__(self):
        """Validate that start is before end."""
        assert self.start <= self.end, f"Start must be less than end. {self.start} !< {self.end}"

    @staticmethod
    def from_match(match: Match) -> SpanT:
        """Span from Match."""
        start, end = match.span()
        return Span(start, end)  # type: ignore[return-value]
        # False positive on abstract type.

    def add_start(self, new_span: SpanT) -> SpanT:
        """Span add start from new span."""
        return Span(self.start + new_span.start, self.end + new_span.start)  # type: ignore[return-value]
        # False positive on abstract type.

    def to_string(self, quotes: str) -> str:
        """Get string from span."""
        return quotes[self.start: self.end]


@define
class SpecificationElement(NodeMixin):
    """Either a Specification file or header in a Specification file."""

    level: int = field(init=True, repr=False)
    title: str = field(init=True, repr=True)

    def add_child(self, child: SpecificationElementT):
        """Add a child Specification Header."""
        assert self.level < child.level, f"Child's level: {child.level} is higher than parent's: {self.level}"
        assert len(child.children) == 0, "Cannot add child that has children"
        child.parent = self


@define
class SpecificationHeader(SpecificationElement, metaclass=ABCMeta):
    """Represent a Specification Header.

    Facilitates creating a Header Tree.
    """

    title_span: Span = field(init=False, default=None, repr=False)
    body_span: Span = field(init=False, default=None, repr=False)

    @staticmethod
    @abstractmethod
    def is_header(line: str) -> bool:
        """Detect a header."""

    @staticmethod
    @abstractmethod
    def from_line(line: str) -> SpecificationHeaderT:
        """Generate a Header from a line."""

    @staticmethod
    @abstractmethod
    def from_match(match: Match) -> SpecificationHeaderT:
        """Generate a Header from a re.Match."""

    def set_body(self, span: Span):
        """Set the body span."""
        self.body_span = span

    def get_body(self) -> str:
        """Get the body of the header."""
        assert hasattr(self.root, "content"), "Cannot call get_body if self.root has no content attribute"
        assert isinstance(self.body_span, Span), "Cannot call get_body if self.body_span is not set"
        return self.root.content[self.body_span.start: self.body_span.end]

    def get_url(self) -> str:
        """Prefixes parent titles to this title.

        Titles are transformed as follows:
        - spaces are replaced with "-"
        - "." are replaced with "_"
        """
        url: str = self.title.replace(" ", "-").replace(".", "_")
        header_cursor: SpecificationHeader = self.parent
        while header_cursor is not None:
            cursor_url = header_cursor.title.replace(" ", "-").replace(".", "_")
            url = ".".join([cursor_url, url])
            header_cursor = header_cursor.parent
        return url

    def validate(self) -> bool:
        """Check that all needed fields are set and reasonable."""
        # fmt: off
        return (self.body_span is not None
                and self.title_span is not None
                and len(self.root.content) >= self.body_span.end
                )
        # fmt: on


@define
class ParsedSpecification(SpecificationElement, metaclass=ABCMeta):
    """Represent a Specification.

    Creates a tree from the file's headers,
    with itself as the root of the tree.

    ParsedSpecification extends NodeMixin,
    so all tree walking methods from anytree are supported.
    In particular, to view all the headers, use `descendants`.
    To check just the top level headers, use `children`.
    """

    filepath: Path = field(init=True, repr=False)
    cursor: Union[SpecificationHeader, ParsedSpecificationT] = field(init=False, repr=False)  # type: ignore[valid-type]
    content: str = field(init=False, repr=False)

    @staticmethod
    @abstractmethod
    def parse(filepath: Path) -> ParsedSpecificationT:
        """Read Specification file and create Header tree."""

    @staticmethod
    @abstractmethod
    def is_file_format(filename: str) -> bool:
        """Detect Specification file."""

    @abstractmethod
    def _match_headers(self) -> Iterator[Match]:
        """Results of a re.Pattern.finditer on self.content."""

    @abstractmethod
    def _new_header(self, match: Match) -> SpecificationHeader:
        """Create a new header element from a match."""

    def __attrs_post_init__(self):
        """Actually Read file and create Header tree."""
        self.cursor = self
        with open(file=self.filepath, mode="rt", encoding="utf-8") as spec:
            self.content = spec.read()
        self._process()

    def _process(self):
        match_iter = self._match_headers()
        for match in match_iter:
            new_header = self._new_header(match)
            self._insert_header(self.cursor, new_header)
            self._set_cursor_body(match)
            self.cursor = new_header
        self._handle_last_header()
        self.reset_cursor()

    def _insert_header(self, cursor: SpecificationElement, new_header: SpecificationHeader):
        """Insert a Header into the Tree.

        This method ASSUMES text is processed serially.
        """
        if cursor.level < new_header.level:
            for child in reversed(cursor.children):
                if child.level < new_header.level:
                    return child.add_child(new_header)
            return cursor.add_child(new_header)
        elif cursor.level >= new_header.level:
            return self._insert_header(cursor.parent, new_header)
        else:
            raise Exception("The logic for ParsedSpecification._insert_header is incorrect.")

    def _set_cursor_body(self, match: Match):
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

    def reset_cursor(self):
        """Reset the cursor to root."""
        self.cursor = self


__all__ = ("Span", "SpecificationHeader", "ParsedSpecification", "MAX_HEADER_LEVELS")
