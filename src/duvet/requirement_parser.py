# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Requirement Parser used by duvet-python."""
import pathlib
import re
import warnings
from typing import List

import attr
from attrs import define, field

from duvet.identifiers import RequirementLevel
from duvet.structures import Requirement, Section

__all__ = ["RequirementParser"]

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

    _legacy: bool = field(init=False, default=False)
    _format: str = field(init=False, default="MARKDOWN")
    _list_entry_regex: re.Pattern = field(init=False, default=ALL_MARKDOWN_LIST_ENTRY_REGEX)
    _filenames: List[pathlib.Path] = field(init=True)

    @classmethod
    def set_legacy(cls):
        """Set legacy mode."""
        cls._legacy = True

    @classmethod
    def set_rfc(cls):
        """Set RFC format."""
        cls._format = "RFC"
        cls._list_entry_regex = ALL_RFC_LIST_ENTRY_REGEX

    def extract_requirements(self, quotes: str) -> list:
        """Take a chunk of string in section.

        Create a list of sentences containing RFC2019 keywords.
        The following assumptions are made about the structure of the In line requirements:
        1. Section string is not included in the string chunk.
        2. There is no list or table within the requirement sring we want to parse
        3. There is no e.g. or ? to break the parser.

        TODO: During these extractions we lost all the location information of the requirements.
        Which would be needed in the report. For now I am gonna ignore it.

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
                temp.extend(req_in_list.to_string_list())
            result.extend(_extract_inline_requirements(quotes[: list_block_left + 2]))
            result.extend(temp)
            result.extend(self.extract_requirements(quotes[list_block_right + 2 :]))
            return result
        else:
            return _extract_inline_requirements(quotes)


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
class ListRequirements:
    """Represent a List of Requirements in the specification.

    Facilitates creating a list of requirement objects in sections.

    :param str list_parent: The sentence right above the list
    :param list list_elements: The word or sentence with a clear sign of ordered or unordered list
    """

    list_parent: str
    list_elements: list = field(init=False, default=attr.Factory(list))

    @classmethod
    def from_line(cls, quotes: str, list_entry_format: re.Pattern):
        """Create list requirements from a chunk of string."""
        # Find the end of the list using the "\n\n".
        end_of_list = quotes.rfind("\n\n") + 2
        # Find the start of the list using the MARKDOWN_LIST_MEMBER_REGEX.
        if re.search(list_entry_format, quotes) is None:
            raise ValueError("Requirement list syntax is not valid in " + quotes)
        first_list_identifier = re.search(list_entry_format, quotes).span()
        start_of_list = first_list_identifier[0]
        list_parent = quotes[0:start_of_list].strip("\n").replace("\n", " ")
        new_list_requirements = cls(list_parent)
        matched_span = []
        prev = first_list_identifier[1]
        for match in re.finditer(list_entry_format, quotes):
            if prev < match.span()[0]:
                temp = quotes[prev : match.span()[0]].strip("\n").replace("\n", " ")
                prev = match.span()[1]
                matched_span.append(temp)
        # last element of th list
        matched_span.append(quotes[prev:end_of_list].strip("\n").replace("\n", " "))
        new_list_requirements.list_elements = matched_span
        return new_list_requirements

    def add_list_element(self, elem: str):
        """Add a list element to the ListRequirement."""
        self.list_elements.append(elem)

    def to_string_list(self) -> list:
        """Convert a ListRequirements Object to a list of string."""
        result = []
        for elem in self.list_elements:
            result.append(" ".join([self.list_parent, elem]))
        return result


def extract_list_requirements(lines: list, start_line: int, end_line: int, list_regex: re.Pattern) -> ListRequirements:
    """Take a List of lines in the specification.

    Creates a list of elements in string.
    """
    list_elements = []
    list_parent = ""
    if not lines[start_line].startswith("\n"):
        list_parent = lines[start_line].strip()
        curr_line = start_line + 1
        curr_list_content = ""
        while curr_line <= end_line:
            if re.match(list_regex, lines[curr_line]):
                curr_list_content = lines[curr_line].strip()
                list_elements.append(curr_list_content)
            elif curr_list_content != "" and len(list_elements) != 0:
                # handle multi-line entries
                curr_list_content = " ".join([curr_list_content, lines[curr_line].strip()])
                list_elements.pop()
                list_elements.append(curr_list_content)
            curr_line += 1

    list_req = ListRequirements(list_parent)
    for elem in list_elements:
        list_req.add_list_element(elem)
    return list_req


def create_requirements_from_list_to_section(section: Section, list_req: list) -> bool:
    """Take a list of string of requirements and Section.

    Creates Requirement Object within that section
    """

    def _create_requirement(level: RequirementLevel, _req_line: str, _section: Section) -> Requirement:
        """Take a RequirementList element and Section.

        Creates Requirement Object within that section
        """
        return Requirement(level, _req_line, _section.uri + "$" + _req_line)

    requirement_list = []
    for req_line in list_req:
        if "MUST" in req_line:
            requirement_list.append(_create_requirement(RequirementLevel.MUST, req_line, section))
        elif "SHOULD" in req_line:
            requirement_list.append(_create_requirement(RequirementLevel.SHOULD, req_line, section))
        elif "MAY" in req_line:
            requirement_list.append(_create_requirement(RequirementLevel.MUST, req_line, section))
        else:
            warnings.warn('No RFC2019 Keywords found in "' + req_line + '"')

    for req in requirement_list:
        section.add_requirement(req)

    return True


def _extract_inline_requirements(quotes: str) -> list:  # pylint: disable too-many-locals
    """Take a chunk of string in section.

    Create a list of sentences containing RFC2019 keywords.

    The following assumptions are made about the structure of the In line requirements:
    1. Each period will be followed by a space, each ! will be followed by a space.
    2. There is no question mark nor ... in the specification chunk trying to parse
    3. There is no list or table within the requirement sTring we want to parse
    4. Section string is not included in the string chunk.
    """
    quotes = _preprocess_inline_requirements(quotes)
    requirement_candidates = []
    requirement_strings = []
    for match in re.finditer(REQUIREMENT_IDENTIFIER_REGEX, quotes):
        requirement_candidates.append(match.span())
    for candidate in requirement_candidates:
        left = candidate[0]
        right = candidate[1]
        sentence_left = 0
        sentence_right = len(quotes) - 1
        left_bound_checked = False
        right_bound_checked = False
        left_punc = quotes[:left].rfind("<stop>")
        if left_punc != -1:
            sentence_left = left_punc
            left_bound_checked = True
        right_punc = quotes[right:].find("<stop>")
        if right_punc != -1:
            right_bound_checked = True
            sentence_right = right + right_punc
        if left_bound_checked and right_bound_checked:
            req = quotes[sentence_left:sentence_right].strip("\n").replace("\n", " ").replace("<stop>", "").strip()
            if req not in requirement_strings and req.endswith((".", "!")):
                requirement_strings.append(req)
    return requirement_strings


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
        .replace("?\n", "?\n<stop>")
        .replace("!\n", "!\n<stop>")
        .replace("<prd>", ".")
    )
    return processed_text
