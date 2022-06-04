# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Methods and classes for parsing Markdown files."""
import os
import re
from typing import List

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


@define
class MarkdownHeader:
    level: int = field(init=True)
    title: str = field(init=True)
    content: str = field(init=True)
    body: str = field(init=False, default="")
    childHeaders: List = field(init=False, default=[])
    parentHeader = field(init=False, default=None)

    @staticmethod
    def is_header(line: str):
        """Detect markdown header."""
        return True if IS_HEADER_REGEX.fullmatch(line) else False

    @staticmethod
    def header_from_line(line: str):
        """Generate a Markdown Header from a line."""
        assert MarkdownHeader.is_header(line)
        hashes, title = line.split(maxsplit=1)
        return MarkdownHeader(level=len(hashes), title=title.strip(), content=line)


@define
class MarkdownSpecification:
    filepath: os.PathLike
    title: str
    lineCursor: int
    headers: List[MarkdownHeader]

    @staticmethod
    def is_markdown(filename: str):
        return filename.rsplit(".", 1)[-1] in ["md"]

    # Parsing Logic:
    # -- Use Regex to find all headers
    # -- Create Header Tree
