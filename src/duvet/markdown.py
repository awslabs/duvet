# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Methods and classes for parsing Markdown files."""
import re
from pathlib import Path
from typing import Iterator, TypeVar

from attr import define

from duvet.specification_parser import MAX_HEADER_LEVELS, ParsedSpecification, Span, SpecificationHeader

# From start of string                                 :: ^
# Match at least 1 up to MAX_HEADER_LEVELS "#"         :: #{1,MAX_HEADER_LEVELS}
# followed by 1 or more white space excluding new line :: [ \t\r]+
# followed by 1 or more not white space                :: [^\s]+
# followed by 0 or more not newline                    :: [^\n]*
HEADER_REGEX = r"(^#{1," + MAX_HEADER_LEVELS + r"}[ \t\r]+[^\s]+[^\n]*)"
# Match A Markdown Header
IS_HEADER_REGEX = re.compile(HEADER_REGEX)
# Match All Markdown Headers
ALL_HEADERS_REGEX = re.compile(HEADER_REGEX, re.MULTILINE)

MarkdownSpecT = TypeVar("MarkdownSpecT", bound="MarkdownSpecification")
MarkdownHeaderT = TypeVar("MarkdownHeaderT", bound="MarkdownHeader")


@define
class MarkdownHeader(SpecificationHeader):
    """Represent a Markdown Header.

    Facilitates creating a Header Tree.
    """

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
        cls: MarkdownHeaderT = MarkdownHeader.from_line(match.string[match.start() : match.end()])
        cls.title_span = Span.from_match(match)
        return cls

    def get_path(self) -> str:
        """Generate a path from the specification name to this title."""
        paths = [node.title for node in self.path]
        # We could safely do this because there MUST be a "specification.md"
        # And there MUST be a "#specification"
        paths = paths[2:]
        return "/".join(paths).replace(" ", "-")


@define
class MarkdownSpecification(ParsedSpecification):
    """Represent a Markdown Specification.

    Creates a tree from the Markdown file's headers,
    with itself as the root of the tree.

    MarkdownSpecification extends anytree.NodeMixin,
    so all tree walking methods from anytree are supported.
    In particular, to view all the headers, use `descendants`.
    To check just the top level headers, use `children`.
    """

    @staticmethod
    def parse(filepath: Path) -> MarkdownSpecT:
        """Read Markdown file and create Header tree."""
        assert MarkdownSpecification.is_file_format(filepath.suffix), f"{filepath} does not end in .md"
        return MarkdownSpecification(filepath=filepath, title=filepath.name, level=0)

    @staticmethod
    def is_file_format(filename: str) -> bool:
        """Detect Markdown file."""
        return filename.rsplit(".", 1)[-1] == "md"

    def _match_headers(self) -> Iterator[re.Match]:
        return ALL_HEADERS_REGEX.finditer(self.content)

    def _new_header(self, match: re.Match) -> SpecificationHeader:
        return MarkdownHeader.from_match(match)


__all__ = ("MarkdownHeader", "MarkdownSpecification")
