# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Requirement Parser used by duvet-python."""
import copy
import logging
import re
from pathlib import Path
from typing import Dict, List, Optional, Tuple, Union

from attrs import define

from duvet.formatter import SENTENCE_DIVIDER, clean_content
from duvet.identifiers import ALL_MARKDOWN_LIST_ENTRY_REGEX, END_OF_LIST, END_OF_SENTENCE, REGEX_DICT, RequirementLevel, \
    ALL_RFC_LIST_ENTRY_REGEX
from duvet.markdown import MarkdownSpecification
from duvet.specification_parser import Span
from duvet.structures import Report, Requirement, Section, Specification


@define
class RequirementParser:
    """The parser of a requirement in a block."""

    @staticmethod
    def _process_section(body: str, annotated_spans: List[Tuple], list_entry_regex: re.Pattern) -> List[dict]:
        """Take a chunk of string in section.

        Return a list of span and types.
        """
        result: list = []
        for annotated_span in annotated_spans:

            if annotated_span[1] == "INLINE":
                result.extend(RequirementParser._process_inline(body, annotated_span[0]))

            if annotated_span[1] == "LIST_BLOCK":
                lists = []
                blocks = RequirementParser._process_list_block(body, annotated_span[0], list_entry_regex)
                for block in blocks:
                    lists.extend(RequirementParser._process_list(body, block, False))
                result.extend(lists)

        return result

    @staticmethod
    def _process_block(body, quote_span: Span, list_entry_regex: re.Pattern) -> List[Tuple]:
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
        quotes = body[quote_span.start: quote_span.end]
        list_match = re.search(list_entry_regex, quotes)

        # Handover to process_inline if no list identifier found.
        if list_match is None:
            result.append((quote_span, "INLINE"))
            return result

        # Identify start of the list block.
        span: Span = Span.from_match(list_match)
        list_block: Span = Span(0, len(quotes))

        for end_sentence_punc in SENTENCE_DIVIDER:
            left_punc = quotes[: span.start].rfind(end_sentence_punc)
            if left_punc != -1:
                list_block.start = max(list_block.start, left_punc)

        # Identify end of the list block.
        end_of_list_match = re.search(END_OF_LIST, quotes[span.end:])
        if end_of_list_match is not None:
            end_of_list_span: Span = Span.from_match(end_of_list_match)
            list_block.end = span.end + end_of_list_span.start

        # First, add requirement string before list.
        result.append((Span(0, list_block.start + 2).add_start(quote_span), "INLINE"))

        # Second, add requirement string from list.
        result.append((Span(list_block.start + 2, list_block.end + 2).add_start(quote_span), "LIST_BLOCK"))

        # Third, add requirement string after list.
        if list_block.end + 2 < quote_span.end:
            new_span = Span(list_block.end + 2, quote_span.end).add_start(quote_span)
            result.extend(RequirementParser._process_block(quotes, new_span, list_entry_regex))

        return result

    @staticmethod
    def _process_inline(body: str, quote_span: Span) -> list[dict]:
        # Split sentences.
        quotes: str = quote_span.to_string(body)
        matches: list = list(re.finditer(END_OF_SENTENCE, quotes))

        if matches is None:
            return []

        spans: list = [Span.from_match(match) for match in matches]
        sentences: list = []

        prev = 0
        for span in spans:
            sentences.append(Span(prev, span.end))
            prev = span.end

        # Append end of the quotes
        if prev < len(quotes) - 1:
            sentences.append(Span(prev, len(quotes) - 1))

        req_kwargs: list[dict] = []

        # Determine which sentence could be a requirement.
        for sentence in sentences:
            words: str = sentence.to_string(quotes)

            level: Optional[RequirementLevel] = RequirementParser._process_requirement_level(words).get(
                "requirement_level"
            )

            if level is None:
                continue

            if clean_content(words).endswith((".", "!")):
                req_kwarg: dict = {
                    "content": clean_content(words),
                    "span": sentence.add_start(quote_span),
                    "requirement_level": level,
                }
                if req_kwarg not in req_kwargs:
                    req_kwargs.append(req_kwarg)

        return req_kwargs

    @staticmethod
    def _process_list_block(body: str, quote_span: Span, list_entry_regex: re.Pattern) -> list[Dict]:
        """Create list requirements from a chunk of string."""
        quotes = body[quote_span.start: quote_span.end]
        result: list[Dict] = []

        # Find the end of the list using the END OF LIST.
        end_of_list = len(quotes) - 1
        end_of_list_match = re.search(END_OF_LIST, quotes)
        if end_of_list_match is not None:
            end_of_list_span: Span = Span.from_match(end_of_list_match)
            end_of_list = end_of_list_span.start + 2

            quotes = body[quote_span.start: quote_span.start + end_of_list]

        # Find the start of the list using the MARKDOWN_LIST_MEMBER_REGEX.

        # //= compliance/duvet-specification.txt#2.2.2
        # //# List elements MAY contain a period (.) or exclamation point (!)
        # //# and this punctuation MUST NOT terminate the requirement by excluding the following
        # //# elements from the list of requirements.
        list_entry: Optional[re.Match[str]] = re.search(list_entry_regex, quotes)
        if list_entry is None:
            logging.warning("Requirement list syntax is not valid in %s", quotes)
            return result

        first_list_identifier: Span = Span.from_match(list_entry)
        list_parent: Span = Span(0, first_list_identifier.start).add_start(quote_span)

        # Extract children.
        matched_span = []
        prev = first_list_identifier.end
        for match in re.finditer(list_entry_regex, quotes):
            if prev < match.span()[0]:
                temp = Span(prev, match.span()[0]).add_start(quote_span)
                prev = match.span()[1]
                matched_span.append(temp)

        # Append the last element of the list
        matched_span.append(Span(prev, end_of_list).add_start(quote_span))

        result.append({"parent": list_parent, "children": matched_span})
        return result

    @staticmethod
    def _process_list(body: str, kwargs: Dict, is_legacy: bool) -> list[Dict]:
        """Give a dictionary of keyword arguments.

        Return a list of dictionaries.

        Param:
            kwarg = {"parent": "parent_sentence", "children": [child1, child2]}
        Return:
            [ "parent_sentence child1", "parent_sentence child2" ]
        """
        req_list: list[Dict] = []

        # Parent MUST NOT be None
        parent: Span = kwargs.get("parent")  # type: ignore[assignment]
        parent_string: str = parent.to_string(body)
        parent_level = RequirementParser._process_requirement_level(parent_string)

        if parent_level.get("requirement_level") is None:
            return []

        # There MUST be parent.
        if is_legacy:
            # quotes = parent.to_string(body)
            req_kwarg: dict = {
                "content": clean_content(parent_string),
                "span": parent,
            }
            req_kwarg.update(RequirementParser._process_requirement_level(parent_string))
            req_list.append(req_kwarg)
            return req_list

        # Children MUST NOT be None
        children: list = kwargs.get("children")  # type: ignore[assignment]

        # There MUST be children.
        for child in children:
            quotes = " ".join([clean_content(parent.to_string(body)), clean_content(child.to_string(body))])
            child_kwarg: dict = {"span": child, "content": clean_content(quotes)}
            child_kwarg.update(parent_level)

            req_list.append(child_kwarg)
        return req_list

    @staticmethod
    def _process_requirement_level(req_line) -> dict:
        """Get requirement level."""

        level: Optional[RequirementLevel] = None

        # //= compliance/duvet-specification.txt#2.2.2
        # //= type=implication
        # //# Any complete sentence containing at least one RFC 2119 keyword MUST be treated as a requirement.
        # //= compliance/duvet-specification.txt#2.2.2
        # //= type=implication
        # //# A requirement MAY contain multiple RFC 2119 keywords.

        if "MAY" in req_line:
            level = RequirementLevel.MAY

        if "SHOULD" in req_line:
            level = RequirementLevel.SHOULD

        if "MUST" in req_line:
            level = RequirementLevel.MUST

        if level is None:
            logging.info("No RFC2019 Keywords found in %s", req_line)

        return {"requirement_level": level}

    @staticmethod
    def process_specifications(filepaths: list[Path], report: Optional[Report] = None) -> Report:
        """Given pattern and filepath of markdown specs.

        Return or create a report.
        """
        if report is None:
            report = Report()

        specifications: list[Specification] = []
        for filepath in filepaths:
            specifications.append(RequirementParser._process_specification(filepath))

        for specification in specifications:
            report.add_specification(specification)

        return report

    @staticmethod
    def _process_specification(specification_source: Path) -> Specification:  # pylint:disable=R0914
        """Given a filepath of a markdown spec.

        Return a specification or none.
        """

        # if specification_source.suffix == ".txt":
        #     parser: RFCSpecification = RFCSpecification.parse(specification_source)

        parser: Union[None, MarkdownSpecification] = None

        if specification_source.endswith(".md"):
            parser = MarkdownSpecification.parse(specification_source)

        specification = Specification(
            specification_source.name, str(specification_source.relative_to(specification_source.parent.parent))
        )

        for section in RequirementParser._process_sections(parser, specification_source):
            if specification is not None:
                specification.add_section(section)

        return specification

    @staticmethod
    def _process_sections(parser, filepath) -> List[Section]:

        sections: list[Section] = []

        for descendant in parser.descendants:
            start_line = parser.content[: descendant.body_span.start].count("\n")
            end_line = parser.content[: descendant.body_span.end].count("\n")
            quotes: str = copy.deepcopy(descendant.get_body())

            lines = quotes.splitlines()
            lines[0] = "   ".join([descendant.number, descendant.title])

            section_kwarg: dict = {
                "title": descendant.number.rstrip(". "),
                "start_line": start_line,
                "end_line": end_line,
                "lines": lines,
                "uri": "#".join([str(filepath.relative_to(filepath.parent.parent)), descendant.number.rstrip(". ")]),
            }

            section = Section(**section_kwarg)

            section_with_requirements: list[Section] = []
            # if filepath.suffix == ".txt":
            #     section_with_requirements.append(RequirementParser._process_requirements(quotes, section,
            #                                                                              ALL_RFC_LIST_ENTRY_REGEX)

            if filepath.suffix == ".md":
                section_with_requirements.append(
                    RequirementParser._process_requirements(quotes, section, ALL_MARKDOWN_LIST_ENTRY_REGEX))

            sections.extend(section_with_requirements)

        return sections

    @staticmethod
    def _process_requirements(quotes, section, regex: re.Pattern) -> Section:

        blocks = RequirementParser._process_block(
            quotes, Span(0, len(quotes)), regex)

        req_kwargs: List[dict] = RequirementParser._process_section(
            quotes, blocks, regex)

        for kwarg in req_kwargs:
            content: Optional[str] = kwarg.get("content")
            if content is not None:
                kwarg.setdefault("uri", "$".join([section.uri, content]))
            if "span" in kwarg.keys():
                kwarg.pop("span")
            section.add_requirement(Requirement(**kwarg))

        return section

# //= compliance/duvet-specification.txt#2.2.2
# //= type=implication
# //# A requirement MUST be terminated by one of the following:

# //= compliance/duvet-specification.txt#2.2.2
# //= type=implication
# //# In the case of requirement terminated by a list, the text proceeding the list MUST be concatenated with each
# //# element of the list to form a requirement.

# //= compliance/duvet-specification.txt#2.2.2
# //= type=implication
# //# List elements MAY have RFC 2119 keywords, this is the same as regular sentences with multiple keywords.


# //= compliance/duvet-specification.txt#2.3.6
# //= type=implication
# //# A one or more line meta part MUST be followed by at least a one line content part.

# //= compliance/duvet-specification.txt#2.2.2
# //= type=TODO
# //# Sublists MUST be treated as if the parent item were terminated by the sublist.


# //= compliance/duvet-specification.txt#2.2.1
# //# The name of the sections MUST NOT be nested.

# //= compliance/duvet-specification.txt#2.2.1
# //= type=exception
# //# A requirements section MUST be the top level containing header.

# //= compliance/duvet-specification.txt#2.2.1
# //= type=implication
# //# A header MUST NOT itself be a requirement.

# //= compliance/duvet-specification.txt#2.2.1
# //= type=TODO
# //# A section MUST be indexable by combining different levels of naming.

# //= compliance/duvet-specification.txt#2.2.2
# //= type=TODO
# //# Sublists MUST be treated as if the parent item were
# //# terminated by the sublist.
