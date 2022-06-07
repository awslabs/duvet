import re

import attr
from attrs import define, field

from duvet.identifiers import *
from duvet.structures import Requirement, Section


@define
class ListRequirements:
    """Represent a List of Requirements in the specification.

    Facilitates creating a list of requirement objects in sections.

    :param str list_parent: The sentence right above the list
    :param list list_elements: The word or sentence with a clear sign of ordered or unordered list
    """

    list_parent: str
    list_elements: list = field(init=False, default=attr.Factory(list))

    def add_list_element(self, elem: str):
        self.list_elements.append(elem)


def extract_list_requirements(lines: list, start_line: int, end_line: int, list_regex) -> ListRequirements:
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
    section_line = list_req.list_parent
    requirement_list = []
    if "MUST" in section_line:
        for child in list_req.list_elements:
            curr_requirement = Requirement(
                RequirementLevel.MUST,
                " ".join([section_line, child]),
                section.uri + "$" + " ".join([section_line, child]),
            )
            requirement_list.append(curr_requirement)
    elif "SHOULD" in section_line:
        for child in list_req.list_elements:
            curr_requirement = Requirement(
                RequirementLevel.SHOULD,
                " ".join([section_line, child]),
                section.uri + "$" + " ".join([section_line, child]),
            )
            requirement_list.append(curr_requirement)
    elif "MAY" in section_line:
        for child in list_req.list_elements:
            curr_requirement = Requirement(
                RequirementLevel.SHOULD,
                " ".join([section_line, child]),
                section.uri + "$" + " ".join([section_line, child]),
            )
            requirement_list.append(curr_requirement)
    else:
        return False

    for req in requirement_list:
        section.add_requirement(req)

    return True
