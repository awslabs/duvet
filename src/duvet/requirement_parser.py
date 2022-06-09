# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Requirement Parser used by duvet-python."""
import re

import attr
from attrs import define, field

from duvet.identifiers import RequirementLevel
from duvet.structures import Requirement, Section

MARKDOWN_LIST_MEMBER_REGEX = r"(^(?:(?:(?:\-|\+|\*)|(?:(\d)+\.)) ))"
# Match All List identifiers
ALL_MARKDOWN_LIST_ENTRY_REGEX = re.compile(MARKDOWN_LIST_MEMBER_REGEX, re.MULTILINE)

RFC_LIST_MEMBER_REGEX = r"(^(?:(\s)*((?:(\-|\*))|(?:(\d)+\.)|(?:[a-z]+\.)) ))"
# Match All List identifier
ALL_RFC_LIST_ENTRY_REGEX = re.compile(RFC_LIST_MEMBER_REGEX, re.MULTILINE)
# Match common List identifiers
# INVALID_LIST_MEMBER_REGEX = r"^(?:(\s)*((?:(\+))|(?:(\()*(\d)+(\))+\.)|(?:(\()*[a-z]+(\))+\.)) )"

END_OF_LIST = r"\n\n"
FIND_ALL_MARKDOWN_LIST_ELEMENT_REGEX = re.compile(r"(^(?:(?:(?:\-|\+|\*)|(?:(\d)+\.)) ))(.*?)", re.MULTILINE)

SENTENCE_DIVIDER = [". ", "! ", ".\n", "!\n"]


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
    def from_line(cls, quotes: str):
        """Create list requirements from a chunk of string."""

        # Find the end of the list using the "\n\n".
        end_of_list = re.search(re.compile(r"[\r\n]{2}", re.MULTILINE), quotes).span()[1]
        # Find the start of the list using the MARKDOWN_LIST_MEMBER_REGEX.
        first_list_identifier = re.search(ALL_MARKDOWN_LIST_ENTRY_REGEX, quotes).span()
        start_of_list = first_list_identifier[0]
        list_parent = quotes[0:start_of_list].strip("\n").replace("\n", " ")
        new_list_requirements = cls(list_parent)
        matched_span = []
        prev = first_list_identifier[1]
        for match in re.finditer(ALL_MARKDOWN_LIST_ENTRY_REGEX, quotes):
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


def create_requirements_from_list(section: Section, list_req: ListRequirements) -> bool:
    """Take a RequirementList and Section.

    Creates Requirement Object within that section
    """

    def _create_requirement(
        level: RequirementLevel, _section_line: str, _list_entry: str, _section: Section
    ) -> Requirement:
        """Take a RequirementList element and Section.

        Creates Requirement Object within that section
        """
        return Requirement(
            level, " ".join([_section_line, _list_entry]), _section.uri + "$" + " ".join([_section_line, _list_entry])
        )

    section_line = list_req.list_parent
    requirement_list = []
    if "MUST" in section_line:
        for child in list_req.list_elements:
            requirement_list.append(_create_requirement(RequirementLevel.MUST, section_line, child, section))
    elif "SHOULD" in section_line:
        for child in list_req.list_elements:
            requirement_list.append(_create_requirement(RequirementLevel.SHOULD, section_line, child, section))
    elif "MAY" in section_line:
        for child in list_req.list_elements:
            requirement_list.append(_create_requirement(RequirementLevel.MUST, section_line, child, section))
    else:
        return False

    for req in requirement_list:
        section.add_requirement(req)

    return True


REQUIREMENT_IDENTIFIER_REGEX = re.compile(r"(MUST|SHOULD|MAY)", re.MULTILINE)


def extract_inline_requirements(quotes: str) -> list:
    """Take a chunk of string in section.

    Create a list of sentences containing RFC2019 keywords.

    The following assumptions are made about the structure of the In line requirements:
    1. Each period will be followed by a space, each ! will be followed by a space.
    2. There is no question mark nor ... in the specification chunk trying to parse
    3. There is no list or table within the requirement sTring we want to parse
    4. Section string is not included in the string chunk.
    """
    requirement_candidates = []
    requirement_spans = []
    requirement_strings = []
    # We don't want to take care of list in this function.
    # We will help get the first sentence of the list and
    # get rid of it.
    for match in re.finditer(REQUIREMENT_IDENTIFIER_REGEX, quotes):
        requirement_candidates.append(match.span())
    for candidate in requirement_candidates:
        left = candidate[0]
        right = candidate[1]
        left_bound_checked = False
        right_bound_checked = False
        while left > 0:
            left = left - 1
            if quotes[left : left + 2] in [". ", "! ", ".\n", "!\n"]:
                left_bound_checked = True
                break
        while right < len(quotes) - 1:
            right = right + 1
            if quotes[right : right + 2] in [". ", "! ", ".\n", "!\n"]:
                right_bound_checked = True
                break
        if left_bound_checked and right_bound_checked:
            temp_span = (left + 2, right + 1)
            if temp_span not in requirement_spans:
                requirement_spans.append(temp_span)
    for req in requirement_spans:
        requirement_strings.append(quotes[req[0] : req[1]].strip("\n").replace("\n", " "))
    return requirement_strings


def extract_requirements(quotes: str) -> list:
    """Take a chunk of string in section.

    Create a list of sentences containing RFC2019 keywords.
    The following assumptions are made about the structure of the In line requirements:
    1. Section string is not included in the string chunk.
    2. There is no list or table within the requirement sring we want to parse
    3. There is no e.g. or ? to break the parser.

    TODO: During these extractions we lost all the location information of the requirements.
    TODO: Which would be needed in the report. For now I am gonna ignore it.
    """
    temp_match = re.search(ALL_MARKDOWN_LIST_ENTRY_REGEX, quotes)
    result = []
    temp = []
    if temp_match is not None:
        left = temp_match.span()[0]
        right = temp_match.span()[1]
        left_bound_checked = False
        right_bound_checked = False
        while left > 0:
            left = left - 1
            if quotes[left : left + 2] in [". ", "! ", ".\n", "!\n"]:
                left_bound_checked = True
                break
        while right < len(quotes) - 1:
            right = right + 1
            if quotes[right : right + 2] in ["\n\n"]:
                right_bound_checked = True
                break
        if left_bound_checked and right_bound_checked:
            # Call the function to take care of the lis of requirements
            req_in_list = ListRequirements.from_line(quotes[left + 2 : right + 1])
            # print(req_in_list.to_string_list())
            temp.extend(req_in_list.to_string_list())
        result.extend(extract_inline_requirements(quotes[0 : left + 2]))
        result.extend(temp)
        result.extend(extract_requirements(quotes[right + 2 : len(quotes) - 1]))
        return result
    else:
        return extract_inline_requirements(quotes)
