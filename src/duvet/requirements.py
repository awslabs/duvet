# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Requirement Parser used by duvet-python."""
import logging
import re
import warnings
from typing import List, Optional

import attr
from attrs import define, field

from duvet.identifiers import RequirementLevel
from duvet.structures import Requirement, Section

# __all__ = ["RequirementParser"]

MARKDOWN_LIST_MEMBER_REGEX = r"(^(?:(?:(?:\-|\+|\*)|(?:(\d)+\.)) ))"
# Match All List identifiers
ALL_MARKDOWN_LIST_ENTRY_REGEX = re.compile(MARKDOWN_LIST_MEMBER_REGEX, re.MULTILINE)

RFC_LIST_MEMBER_REGEX = r"(^(?:(\s)*((?:(\-|\*))|(?:(\d)+\.)|(?:[a-z]+\.)) ))"
# Match All List identifier
ALL_RFC_LIST_ENTRY_REGEX = re.compile(RFC_LIST_MEMBER_REGEX, re.MULTILINE)
# Match common List identifiers
# INVALID_LIST_MEMBER_REGEX = r"^(?:(\s)*((?:(\+))|(?:(\()*(\d)+(\))+\.)|(?:(\()*[a-z]+(\))+\.)) )"
REQUIREMENT_IDENTIFIER_REGEX = re.compile(r"(MUST|SHOULD|MAY)", re.MULTILINE)
END_OF_LIST = r"\n\n"
FIND_ALL_MARKDOWN_LIST_ELEMENT_REGEX = re.compile(r"(^(?:(?:(?:\-|\+|\*)|(?:(\d)+\.)) ))(.*?)", re.MULTILINE)
# Common sentence dividers
SENTENCE_DIVIDER = [". ", "! ", ".\n", "!\n", "? ", "?\n"]
# Common sentence dividers would mix up words
ALPHABETS = r"([A-Za-z])"
PREFIXES = r"(Mr|St|Mrs|Ms|Dr)[.]"
SUFFIXES = r"(Inc|Ltd|Jr|Sr|Co)"
STARTERS = r"(Mr|Mrs|Ms|Dr|He\s|She\s|It\s|They\s|Their\s|Our\s|We\s|But\s|However\s|That\s|This\s|Wherever)"
ACRONYMS = r"([A-Z][.][A-Z][.](?:[A-Z][.])?)"
WEBSITES = r"[.](com|net|org|io|gov)"


@define
class RequirementParser:
    """The parser of a requirement in a block."""

    is_legacy: bool = field(init=False, default=False)
    _format: str = field(init=False, default="MARKDOWN")
    _list_entry_regex: re.Pattern = field(init=False, default=ALL_MARKDOWN_LIST_ENTRY_REGEX)

    @classmethod
    def set_legacy(cls):
        """Set legacy mode."""
        cls.is_legacy = True

    @classmethod
    def set_rfc(cls):
        """Set RFC format."""
        cls._format = "RFC"
        cls._list_entry_regex = ALL_RFC_LIST_ENTRY_REGEX

    def extract_requirements(self, quotes: str) -> List[str]:
        """Take a chunk of string in section.

        Create a list of sentences containing RFC2019 keywords.
        The following assumptions are made about the structure of the In line requirements:
        1. Section string is not included in the string chunk.
        2. There is no list or table within the requirement sring we want to parse
        3. There is no e.g. or ? to break the parser.

        list block is considered as a block of string. It starts with a sentence, followed by ordered
        or unordered lists. It end with two nextline signs
        """
        temp_match = re.search(self._list_entry_regex, quotes)
        result = []
        temp = []
        if temp_match is not None:
            left = temp_match.span()[0]
            right = temp_match.span()[1]
            list_block_left = 0
            list_block_right = len(quotes) - 1
            left_bound_checked = False
            right_bound_checked = False
            for end_sentence_punc in SENTENCE_DIVIDER:
                left_punc = quotes[:left].rfind(end_sentence_punc)
                if left_punc != -1:
                    left_bound_checked = True
                    list_block_left = max(list_block_left, left_punc)
            right_punc = quotes[right:].find("\n\n")
            if right_punc != -1:
                right_bound_checked = True
                list_block_right = right + right_punc
            if left_bound_checked and right_bound_checked:
                # Call the function to take care of the lis of requirements
                req_in_list = ListRequirements.from_line(
                    quotes[list_block_left + 2 : list_block_right + 2], self._list_entry_regex
                )
                temp.extend(req_in_list.to_string_list(self.is_legacy))
            result.extend(_extract_inline_requirements(quotes[: list_block_left + 2]))
            result.extend(temp)
            result.extend(self.extract_requirements(quotes[list_block_right + 2 :]))
            return result
        else:
            return _extract_inline_requirements(quotes)

    def process_section(self):
        pass

    def process_inline(self, kwargs: dict) -> list[str]:
        # pass
        reqs = []

    def process_list(self, kwargs: dict) -> list[str]:
        """


        input: kwarg = {"parent": "parent_sentence", "children": [child1, child2]}
        output: [ "parent_sentence child1", "parent_sentence child2" ]
        """
        req_list = []
        parent: Optional[str] = kwargs.get("parent")
        if parent is None:
            logging.WARNING("no list parent")
            return []
        elif self.is_legacy:
            return req_list.append(parent)
        children: Optional[str] = kwargs.get("children")
        if children is None:
            logging.WARNING("no list children")
            return
        else:
            for child in children:
                req_list.append(" ".join([parent, child]))
        return req_list


def _preprocess_inline_requirements(inline_text: str) -> str:
    """Take a chunk of inline requirement string and return a labeled string."""
    processed_text = "<stop> " + inline_text + "  <stop>"
    processed_text = processed_text.replace("\n", " ")
    processed_text = re.sub(PREFIXES, "\\1<prd>", processed_text)
    processed_text = re.sub(WEBSITES, "<prd>\\1", processed_text)
    if "Ph.D" in processed_text:
        processed_text = processed_text.replace("Ph.D.", "Ph<prd>D<prd>")
    processed_text = re.sub(r"\s" + ALPHABETS + "[.] ", " \\1<prd> ", processed_text)
    processed_text = re.sub(ACRONYMS + " " + STARTERS, "\\1<stop> \\2", processed_text)
    processed_text = re.sub(
        ALPHABETS + "[.]" + ALPHABETS + "[.]" + ALPHABETS + "[.]", "\\1<prd>\\2<prd>\\3<prd>", processed_text
    )
    processed_text = re.sub(ALPHABETS + "[.]" + ALPHABETS + "[.]", "\\1<prd>\\2<prd>", processed_text)
    processed_text = re.sub(" " + SUFFIXES + "[.] " + STARTERS, " \\1<stop> \\2", processed_text)
    processed_text = re.sub(" " + SUFFIXES + "[.]", " \\1<prd>", processed_text)
    processed_text = re.sub(" " + ALPHABETS + "[.]", " \\1<prd>", processed_text)
    if "”" in processed_text:
        processed_text = processed_text.replace(".”", "”.")
    if '"' in processed_text:
        processed_text = processed_text.replace('."', '".')
    if "!" in processed_text:
        processed_text = processed_text.replace('!"', '"!')
    if "?" in processed_text:
        processed_text = processed_text.replace('?"', '"?')
    processed_text = (
        processed_text.replace(". ", ". <stop>")
        .replace("? ", "? <stop>")
        .replace("! ", "! <stop>")
        .replace(".\n", ".\n<stop>")
        .replace("?\n", "?\n<stop>")  # noqa: E131
        .replace("!\n", "!\n<stop>")
        .replace("<prd>", ".")
    )
    return processed_text
