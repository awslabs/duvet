# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Methods and classes for parsing RFC files."""
import re
from pathlib import Path
from typing import Iterator, TypeVar

from attr import define, field

from duvet.specification_parser import MAX_HEADER_LEVELS, ParsedSpecification, Span, SpecificationHeader

# Capture                                                       :: (
# first non-capture                                             :: (?:
# one or more digits and a period                               :: \d+\.
# first non-capture at least one up to MAX_HEADER_LEVELS times  :: ){1,MAX_HEADER_LEVELS}
# followed by exactly 2 spaces                                  :: [ ]{2}
# second non-capture group                                      :: (?:
# one or more non-whitespace                                    :: \S+
# none or more other than new-line white-space                  :: [ \t\r]*
# second non-capture at least one or more times                 :: )+
# close Capture                                                 :: )
HEADER_REGEX = r"((?:\d+\.){1," + MAX_HEADER_LEVELS + r"}[ ]{2}(?:\S+[ \t\r]*)+)"
IS_HEADER_REGEX = re.compile(HEADER_REGEX)
# Header lines have no white space before them, so a line will be the
# start of the string, and the header will encompass the whole line
ALL_HEADERS_REGEX = re.compile(r"^" + HEADER_REGEX + r"$", re.MULTILINE)

RFCSpecT = TypeVar("RFCSpecT", bound="RFCSpecification")
RFCHeaderT = TypeVar("RFCHeaderT", bound="RFCHeader")


@define
class RFCHeader(SpecificationHeader):
    """Represent an RFC Header."""

    number: str = field(init=True, repr=False)

    @staticmethod
    def is_header(line: str) -> bool:
        """Detect RFC header."""
        return bool(IS_HEADER_REGEX.fullmatch(line))

    @staticmethod
    def from_line(line: str) -> RFCHeaderT:
        """Generate an RFC Header from a line."""
        assert RFCHeader.is_header(line), f"line is not an RFC header: {line}"
        # str.split will split on whitespace, breaking digits from everything else
        number, title = line.split(maxsplit=1)
        level = len(number.split("."))
        return RFCHeader(level=level, title=title.strip(), number=number)

    @staticmethod
    def from_match(match: re.Match) -> RFCHeaderT:
        """Generate an RFC Header from a match."""
        cls: RFCHeaderT = RFCHeader.from_line(match.string[match.start(): match.end()])
        cls.title_span = Span.from_match(match)
        return cls


@define
class RFCSpecification(ParsedSpecification):
    """Represent an RFC Specification.

    Creates a tree from the RFC file's headers,
    with itself as the root of the tree.

    RFCSpecification extends anytree.NodeMixin,
    so all tree walking methods from anytree are supported.
    In particular, to view all the headers, use `descendants`.
    To check just the top level headers, use `children`.
    """

    @staticmethod
    def parse(filepath: Path) -> RFCSpecT:
        """Read an RFC file and create Header tree."""
        assert RFCSpecification.is_file_format(filepath.suffix), f"{filepath} does not end in .txt"
        return RFCSpecification(filepath=filepath, title=filepath.name, level=0)

    @staticmethod
    def is_file_format(filename: str) -> bool:
        """Detect RFC File."""
        return filename.rsplit(".", maxsplit=1)[-1] == "txt"

    def _match_headers(self) -> Iterator[re.Match]:
        return ALL_HEADERS_REGEX.finditer(self.content)

    def _new_header(self, match: re.Match) -> RFCHeader:
        return RFCHeader.from_match(match)


__all__ = ("RFCHeader", "RFCSpecification")
