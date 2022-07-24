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
from duvet.formatter import clean_content
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

    paths: list[Path] = field(init=True, default=attr.Factory(list))
    # TODO: Sanitize user input for regular expression usage;
    # //= compliance/duvet-specification.txt#2.3.1
    # //= type=implication
    # //# This identifier of meta parts MUST
    # //# be configurable.
    meta_style: str = field(init=True, default=DEFAULT_META_STYLE)
    content_style: str = field(init=True, default=DEFAULT_CONTENT_STYLE)
    annotations: list[Annotation] = field(init=False, default=attr.Factory(list), repr=False)
    is_anno: re.Pattern = field(init=False, repr=False)
    match_url: re.Pattern = field(init=False, repr=False)
    match_type: re.Pattern = field(init=False, repr=False)
    match_reason: re.Pattern = field(init=False, repr=False)
    match_content: re.Pattern = field(init=False, repr=False)

    def __attrs_post_init__(self):
        """Set regular expression attributes."""
        pattern: str = r"^([\s]*" + f"((?:{self.meta_style})|(?:{self.content_style})))"
        self.is_anno = re.compile(pattern)
        # //= compliance/duvet-specification.txt#2.3.2
        # //# The first line of the meta part identifies the location of the content, it MUST be parsed as a URL.
        self.match_url = re.compile(r"[\s]*" + self.meta_style + r"[\s](.*?)\n")
        self.match_type = re.compile(r"[\s]*" + self.meta_style + r"[\s]type=(.*?)\n")
        # //= compliance/duvet-specification.txt#2.3.4
        # //# It MUST start with "reason=".
        # //= compliance/duvet-specification.txt#2.3.4
        # //= type=implication
        # //# A third meta line MAY exist: Reason.
        # //= compliance/duvet-specification.txt#2.3.4
        # //= type=implication
        # //# The rest of this line and the following meta lines MUST be
        # //#parsed as the annotation's reason until there are no more meta lines.
        self.match_reason = re.compile(r"[\s]*" + self.meta_style + r"[\s]reason=(.*?)\n")
        self.match_content = re.compile(r"[\s]*" + self.content_style + r"[\s]*(.*?)\n")

    def _extract_spans(self, lines: list[str]) -> list[LineSpan]:
        """Extract Annotation spans from a file."""
        spans: list[LineSpan] = []
        start: Optional[int] = None

        for index, line in enumerate(lines):
            anno_hit: Optional[re.Match] = self.is_anno.search(line)
            if anno_hit is None and start is not None:
                spans.append(LineSpan(start=start, end=index))
                start = None
            elif anno_hit is not None and start is None:
                start = index
        # Edge case for annotation blocks that end the file
        if start is not None:
            spans.append(LineSpan(start=start, end=len(lines)))

        return spans

    def _extract_anno_kwargs(self, lines: list[str], spans: list[LineSpan]) -> list[dict]:
        """Parse none or more Annotation key word args from lines via LineSpans."""
        kwargs: list[dict] = []
        for span in spans:
            index: int = span.start
            while index < span.end:
                start: int = index

                # //= compliance/duvet-specification.txt#2.3.2
                # //# All parts of the URL other than a URL fragment MUST be optional and MUST
                # //# identify the specification that contains this section and content.
                # //= compliance/duvet-specification.txt#2.3.2
                # //# The URL MUST contain a URL fragment that uniquely identifies the section
                # //# that contains this content.
                # the first line will be the url
                match = self.match_url.match(lines[index])
                url: Optional[str] = match.__getitem__(1) if isinstance(match, re.Match) else None
                index += 1 if url is not None else 0
                del match

                # //= compliance/duvet-specification.txt#2.3.3
                # //# If the meta part is a single line then the type MUST be citation.
                # //= compliance/duvet-specification.txt#2.3.3
                # //# If a second meta line exists it MUST start with "type=".
                # //= compliance/duvet-specification.txt#2.3.3
                # //# The type MUST be a valid annotation type string:
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
                if not lines[index].endswith("\n"):
                    lines[index] = lines[index] + "\n"
                match = self.match_content.match(lines[index])
                while index < span.end and isinstance(match, re.Match):
                    content += match.__getitem__(1) + "\n"
                    index += 1
                    match = self.match_content.match(lines[index]) if index < span.end else None
                del match

                # fmt: off
                kwarg = {"target": url, "type": _type, "start_line": start,
                         "end_line": index, "reason": reason, "content": clean_content(content)}
                kwargs.append(kwarg)
                # fmt: on
        return kwargs

    @staticmethod
    def _process_anno_kwargs(anno_kwargs: list[dict], filepath: Path) -> list[Annotation]:
        """Convert anno kwargs to Annotations."""
        rtn: list[Annotation] = []
        for kwarg in anno_kwargs:
            if kwarg.get("content") == "" or kwarg.get("target") is None:
                continue
            kwarg["type"] = DEFAULT_ANNO_TYPE_NAME if kwarg["type"] is None else kwarg["type"]
            try:
                kwarg["type"] = AnnotationType[kwarg["type"].upper()]
            except KeyError:
                _LOGGER.warning(
                    "%s: Unknown type: %s found in lines %s to %s. Skipping",
                    filepath,
                    kwarg["type"],
                    kwarg["start_line"],
                    kwarg["end_line"],
                )
                continue
            kwarg["source"] = str(filepath)
            kwarg["uri"] = "$".join([kwarg["target"], kwarg["content"]])
            rtn.append(Annotation(**kwarg))
        return rtn

    def process_file(self, filepath: Path) -> list[Annotation]:
        """Extract annotations from one file."""
        with open(filepath, "r", encoding="utf-8") as implementation_file:
            lines: list[str] = implementation_file.readlines()

        spans: list[LineSpan] = self._extract_spans(lines)
        anno_kwargs: list[dict] = self._extract_anno_kwargs(lines, spans)
        return self._process_anno_kwargs(anno_kwargs, filepath)

    def process_all(self) -> list[Annotation]:
        """Extract annotations from all files."""

        annotations: list[Annotation] = []
        for filepath in self.paths:
            annotations.extend(self.process_file(filepath))
        return annotations


# //= compliance/duvet-specification.txt#2.3.1
# //= type=implication
# //# The default identifier for the meta part in source documents MUST be //= followed by a single space.

# //= compliance/duvet-specification.txt#2.5.3
# //= type=TODO
# //# A specification requirement MUST be labeled "Excused" and MUST only be labeled "Excused" if there exists
# //# a matching annotation of type "exception" and the annotation has a "reason".

# //= compliance/duvet-specification.txt#2.2.3
# //= type=implication
# //# For backwards compatibility Duvet MUST support this older simpler form of requirement identification.

# //= compliance/duvet-specification.txt#2.2.3
# //= type=implication
# //# Any complete sentence containing at least one RFC 2119 keyword MUST be treated as a requirement.

# //= compliance/duvet-specification.txt#2.2.3
# //= type=implication
# //# A requirement MAY contain multiple RFC 2119 keywords.

# //= compliance/duvet-specification.txt#2.2.3
# //= type=implication
# //# A requirement MUST be terminated by one of the following:

# //= compliance/duvet-specification.txt#2.2.3
# //= type=implication
# //# For a given a specification Duvet MUST use one way to identify requirements.

# //= compliance/duvet-specification.txt#2.4.1
# //# For an annotation to match a specification the annotation's
# //# content MUST exist in the specification's section identified by the annotation's meta location URL.

# //= compliance/duvet-specification.txt#2.4.1
# //# The match between the annotation content and the specification text MUST be case-sensitive
# //# but MUST NOT be white space sensitive and MUST uniquely identify text in the specification.

# //= compliance/duvet-specification.txt#2.4.1
# //# Elements of a list MUST NOT be matched by their order within the list.

# //= compliance/duvet-specification.txt#2.4.1
# //= type=exception
# //# Rows of a table MUST NOT be matched by their order within the table.

# //= compliance/duvet-specification.txt#2.4.1
# //= type=exception
# //# This means that an annotation MAY contain a table that is a subset of the rows in the specification.

# //= compliance/duvet-specification.txt#2.6.2
# //= type=TODO
# //# For Duvet to pass the Status of every "MUST" and "MUST NOT" requirement MUST be Complete or Excused.
