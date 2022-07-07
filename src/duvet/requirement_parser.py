# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Requirement Parser used by duvet-python."""
import logging
import re
from typing import Dict, List, Optional, Tuple

from attrs import define, field

from duvet.formatter import SENTENCE_DIVIDER, STOP_SIGN, clean_content, preprocess_text
from duvet.identifiers import RequirementLevel
from duvet.specification_parser import Span

# __all__ = ["RequirementParser"]

MARKDOWN_LIST_MEMBER_REGEX = r"(^(?:(?:(?:\-|\+|\*)|(?:(\d)+\.)) ))"
# Match All List identifiers
ALL_MARKDOWN_LIST_ENTRY_REGEX = re.compile(MARKDOWN_LIST_MEMBER_REGEX, re.MULTILINE)

RFC_LIST_MEMBER_REGEX = r"(^(?:(\s)*((?:(\-|\*))|(?:(\d)+\.)|(?:[a-z]+\.)) ))"
# Match All List identifier
ALL_RFC_LIST_ENTRY_REGEX = re.compile(RFC_LIST_MEMBER_REGEX, re.MULTILINE)
# Match common List identifiers
REQUIREMENT_IDENTIFIER_REGEX = re.compile(r"(MUST|SHOULD|MAY)", re.MULTILINE)
END_OF_LIST = r"\n\n"
FIND_ALL_MARKDOWN_LIST_ELEMENT_REGEX = re.compile(r"(^(?:(?:(?:\-|\+|\*)|(?:(\d)+\.)) ))(.*?)", re.MULTILINE)


@define
class RequirementParser:
    """The parser of a requirement in a block."""

    is_legacy: bool = field(init=False, default=False)
    _format: str = field(init=False, default="MARKDOWN")
    _list_entry_regex: re.Pattern = field(init=False, default=ALL_MARKDOWN_LIST_ENTRY_REGEX)
    # _list_entry_format: re.Pattern = field(init=False, default=MARKDOWN_LIST_MEMBER_REGEX)
    body: str

    @classmethod
    def set_legacy(cls):
        """Set legacy mode."""
        cls.is_legacy = True

    @classmethod
    def set_rfc(cls):
        """Set RFC format."""
        cls._format = "RFC"
        cls._list_entry_regex = ALL_RFC_LIST_ENTRY_REGEX

    def extract_requirements(self, annotated_spans: List[Tuple]) -> List[dict]:
        """Take a chunk of string in section.

        Create a list of sentences containing RFC2019 keywords.
        The following assumptions are made about the structure of the In line requirements:
        1. Section string is not included in the string chunk.
        2. There is no table within the requirement string we want to parse

        list block is considered as a block of string. It starts with a sentence, followed by ordered
        or unordered lists. It ends with two nextline signs

        Method Logic determines if a list is present in the quote_span.
        If there is a list, it determines where the list starts and ends,
        and then invokes helper methods to process the list block into
        requirement keywords. It then invokes itself.
        If there is no list detected,
        it invokes a helper method to convert the quote_span into
        requirement keywords.

        """
        result = []
        for annotated_span in annotated_spans:
            if annotated_span[1] == "INLINE":
                result.extend(self.process_inline(annotated_span[0]))
            if annotated_span[1] == "LIST_BLOCK":
                result.extend(self.process_list_block(annotated_span[0]))
        return result

    def extract_block(self, quote_span: Span) -> List[Tuple]:
        """Take a chunk of string in section.

        Create a list of sentences containing RFC2019 keywords.
        The following assumptions are made about the structure of the In line requirements:
        1. Section string is not included in the string chunk.
        2. There is no table within the requirement string we want to parse

        list block is considered as a block of string. It starts with a sentence, followed by ordered
        or unordered lists. It ends with two nextline signs

        Method Logic determines if a list is present in the quote_span.
        If there is a list, it determines where the list starts and ends,
        and then invokes helper methods to process the list block into
        requirement keywords. It then invokes itself.
        If there is no list detected,
        it invokes a helper method to convert the quote_span into
        requirement keywords.

        """
        result: List = []
        quotes = self.body[quote_span.start : quote_span.end]
        list_match = re.search(self._list_entry_regex, quotes)
        # Handover to process_inline if no list identifier found.
        if list_match is None:
            result.append((quote_span, "INLINE"))
            return result

        # Identify start of the list block.
        span = Span.from_match(list_match)
        list_block = Span(0, len(quotes) - 1)
        left_punc: int = -1
        for end_sentence_punc in SENTENCE_DIVIDER:
            left_punc = quotes[: span.start].rfind(end_sentence_punc)
            if left_punc != -1:
                list_block.start = max(list_block.start, left_punc)

        # Identify end of the list block.
        right_punc = quotes[span.end :].find("\n\n")
        if right_punc != -1:
            list_block.end = span.end + right_punc

        # First, add requirement string before list.
        result.append((Span(0, list_block.start + 2).add_start(quote_span), "INLINE"))

        # Second, add requirement string from list.
        result.append((Span(list_block.start + 2, list_block.end + 2).add_start(quote_span), "LIST_BLOCK"))

        # Third, add requirement string after list.
        result.extend(self.extract_block(Span(list_block.end + 2, quote_span.end).add_start(quote_span)))
        print(result)
        return result

    def process_inline(self, quote_span: Span) -> list[dict]:
        """

        requirement_level: RequirementLevel
        content: str = ""
        uri: str = ""
        span : Span
        """
        quotes = preprocess_text(self.body[quote_span.start : quote_span.end])
        requirement_candidates = []
        req_kwargs = []

        # Find requirement identifiers in the quotes.
        for match in re.finditer(REQUIREMENT_IDENTIFIER_REGEX, quotes):
            requirement_candidates.append(match.span())

        for candidate in requirement_candidates:
            identifier_span = Span(candidate[0], candidate[1])
            sentence_span = Span(0, len(quotes) - 1)

            left_punc = quotes[: identifier_span.start].rfind(STOP_SIGN)
            if left_punc != -1:
                sentence_span.start = left_punc
            right_punc = quotes[identifier_span.end :].find(STOP_SIGN)
            if right_punc != -1:
                sentence_span.end = identifier_span.end + right_punc
            if left_punc != -1 and right_punc != -1:
                req = (
                    quotes[sentence_span.start : sentence_span.end]
                    .strip("\n")
                    .replace("\n", " ")
                    .replace(STOP_SIGN, "")
                    .strip()
                )
                if req.endswith((".", "!")):
                    req_kwarg = {
                        "requirement_level": self.get_requirement_level(quotes),
                        "content": req,
                        "span": sentence_span.add_start(quote_span),
                    }
                    if req_kwarg not in req_kwargs:
                        req_kwargs.append(req_kwarg)

        return req_kwargs

    def process_list_block(self, quote_span: Span) -> Dict:
        """Create list requirements from a chunk of string."""
        quotes = self.body[quote_span.start : quote_span.end]

        print(quotes)
        # Find the end of the list using the "\n\n".
        end_of_list = quotes.rfind("\n\n") + 2

        # Find the start of the list using the MARKDOWN_LIST_MEMBER_REGEX.
        if re.search(self._list_entry_regex, quotes) is None:
            raise ValueError("Requirement list syntax is not valid in " + quotes)

        # Extract parent.
        first_list_identifier = Span.from_match(re.search(self._list_entry_regex, quotes))
        list_parent = Span(0, first_list_identifier.start).add_start(quote_span)

        # Extract children.
        matched_span = []
        prev = first_list_identifier.end
        for match in re.finditer(self._list_entry_regex, quotes):
            if prev < match.span()[0]:
                temp = Span(prev, match.span()[0]).add_start(quote_span)
                prev = match.span()[1]
                matched_span.append(temp)

        # last element of th list
        matched_span.append(Span(prev, end_of_list).add_start(quote_span))

        return {"parent": list_parent, "children": matched_span}

    def process_list(self, kwargs: dict) -> list[dict]:
        """


        input: kwarg = {"parent": "parent_sentence", "children": [child1, child2]}
        output: [ "parent_sentence child1", "parent_sentence child2" ]
        """
        req_list: list[dict] = []
        parent: Optional[Span] = kwargs.get("parent")
        # there MUST be parent.
        if self.is_legacy:
            quotes = parent.to_string(self.body)
            req_kwarg = {
                "requirement_level": self.get_requirement_level(quotes),
                "content": clean_content(quotes),
                "span": parent,
            }
            req_list.append(req_kwarg)
            return req_list
        else:
            children: List[Span] = kwargs.get("children")
            for child in children:
                quotes = " ".join(
                    [clean_content(parent.to_string(self.body)), clean_content(child.to_string(self.body))]
                )
                req_kwarg = {
                    "requirement_level": self.get_requirement_level(quotes),
                    "content": clean_content(quotes),
                    "span": child,
                }
                req_list.append(req_kwarg)
        return req_list

    @staticmethod
    def get_requirement_level(req_line) -> Optional[RequirementLevel]:
        level: Optional[RequirementLevel] = None
        if "MAY" in req_line:
            level = RequirementLevel.MAY
        if "SHOULD" in req_line:
            level = RequirementLevel.SHOULD
        if "MUST" in req_line:
            level = RequirementLevel.MUST
        if level is None:
            logging.warning("No RFC2019 Keywords found in %s", req_line)
        return level
