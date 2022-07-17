# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Requirement Parser used by duvet-python."""
import logging
import os
from pathlib import Path
from re import Match, Pattern, finditer, search
from typing import Dict, List, Optional, Tuple

from attrs import define

from duvet.formatter import SENTENCE_DIVIDER, STOP_SIGN, clean_content, preprocess_text
from duvet.identifiers import ALL_RFC_LIST_ENTRY_REGEX, REGEX_DICT, REQUIREMENT_IDENTIFIER_REGEX, RequirementLevel
from duvet.rfc import RFCSpecification
from duvet.specification_parser import Span
from duvet.structures import Report, Requirement, Section, Specification


@define
class RequirementParser:
    """The parser of a requirement in a block."""

    @staticmethod
    def _process_section(body: str, annotated_spans: List[Tuple], list_entry_regex: Pattern) -> List[dict]:
        """Take a chunk of string in section.

        Return a list of span and types.
        """
        result: list = []
        for annotated_span in annotated_spans:

            if annotated_span[1] == "INLINE":
                result.extend(RequirementParser._process_inline(body, annotated_span[0]))

            if annotated_span[1] == "LIST_BLOCK":
                result.extend(RequirementParser._process_list_block(body, annotated_span[0], list_entry_regex))
        return result

    @staticmethod
    def _process_block(body, quote_span: Span, list_entry_regex: Pattern) -> List[Tuple]:
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
        quotes = body[quote_span.start : quote_span.end]
        list_match = search(list_entry_regex, quotes)

        # Handover to process_inline if no list identifier found.
        if list_match is None:
            result.append((quote_span, "INLINE"))
            return result

        # Identify start of the list block.
        span: Span = Span.from_match(list_match)
        list_block: Span = Span(0, len(quotes) - 1)

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
        new_span = Span(list_block.end + 2, quote_span.end).add_start(quote_span)
        result.extend(RequirementParser._process_block(quotes, new_span, list_entry_regex))
        return result

    @staticmethod
    def _process_inline(body: str, quote_span: Span) -> list[dict]:
        """Given a span of content, return a list of key word arguments of requirement."""

        quotes: str = preprocess_text(body[quote_span.start : quote_span.end])
        requirement_candidates: list = []
        req_kwargs: list = []

        # Find requirement identifiers in the quotes.
        for match in finditer(REQUIREMENT_IDENTIFIER_REGEX, quotes):
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
                req = quotes[sentence_span.start : sentence_span.end]
                req = req.strip("\n")
                req = req.replace("\n", " ")
                req = req.replace(STOP_SIGN, "")
                req = req.strip()

                if req.endswith((".", "!")):
                    req_kwarg = {
                        "content": req,
                        "span": sentence_span.add_start(quote_span),
                    }
                    req_kwarg.update(RequirementParser._process_requirement_level(quotes))
                    if req_kwarg not in req_kwargs:
                        req_kwargs.append(req_kwarg)

        return req_kwargs

    @staticmethod
    def _process_list_block(body, quote_span: Span, _list_entry_regex) -> list[Dict]:
        """Create list requirements from a chunk of string."""
        quotes = body[quote_span.start : quote_span.end]
        result: list[Dict] = []

        # Find the end of the list using the "\n\n".
        end_of_list = quotes.rfind("\n\n") + 2

        # Find the start of the list using the MARKDOWN_LIST_MEMBER_REGEX.
        list_entry: Optional[Match[str]] = search(_list_entry_regex, quotes)
        if list_entry is None:
            logging.warning("Requirement list syntax is not valid in %s", quotes)
            return result

        first_list_identifier: Span = Span.from_match(list_entry)
        list_parent: Span = Span(0, first_list_identifier.start).add_start(quote_span)

        # Extract children.
        matched_span = []
        prev = first_list_identifier.end
        for match in finditer(_list_entry_regex, quotes):
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

        # There MUST be parent.
        if is_legacy:
            quotes = parent.to_string(body)
            req_kwarg: dict = {
                "content": clean_content(quotes),
                "span": parent,
            }
            req_kwarg.update(RequirementParser._process_requirement_level(quotes))
            req_list.append(req_kwarg)
            return req_list

        # Children MUST NOT be None
        children: list = kwargs.get("children")  # type: ignore[assignment]

        # There MUST be children.
        for child in children:
            quotes = " ".join([clean_content(parent.to_string(body)), clean_content(child.to_string(body))])

            child_kwarg: dict = {"span": child, "content": clean_content(quotes)}
            child_kwarg.update(RequirementParser._process_requirement_level(quotes))

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
    def process_specifications(patterns: str, path: Path, report: Optional[Report] = None) -> Report:
        """Given pattern and filepath of markdown specs.

        Return or create a report.
        """
        if report is None:
            report = Report()

        specifications: list[Specification] = []
        for filepath in Path(path).glob(patterns):
            specifications.append(RequirementParser._process_specification(filepath))

        for specification in specifications:
            report.add_specification(specification)

        return report

    @staticmethod
    def _process_specification(specification_source: Path) -> Specification:  # pylint:disable=R0914
        """Given a filepath of a markdown spec.

        Return a specification or none.
        """

        parser: RFCSpecification = RFCSpecification.parse(specification_source)
        specification = Specification(specification_source.name, str(os.path.relpath(specification_source, Path.cwd())))

        for section in RequirementParser._process_sections(parser, specification_source):
            if specification is not None:
                specification.add_section(section)

        return specification

    @staticmethod
    def _process_sections(parser, filepath) -> List[Section]:

        sections: list[Section] = []

        for descendant in parser.descendants:
            # descendant.body_span
            # print( descendant.body_span)
            start_line = parser.content[: descendant.body_span.start].count("\n")
            end_line = parser.content[: descendant.body_span.end].count("\n")
            quotes = descendant.get_body()

            section_kwarg: dict = {
                "title": descendant.get_url(),
                "start_line": start_line,
                "end_line": end_line,
                "quotes": quotes,
                "uri": "#".join([str(filepath.resolve()), descendant.get_url()]),
            }

            section = Section(**section_kwarg)
            sections.append(RequirementParser._process_requirements(quotes, section))

        return sections

    @staticmethod
    def _process_requirements(quotes, section, file_type: str = "RFC") -> Section:
        req_kwargs: List[dict] = RequirementParser._process_section(  # type:ignore[arg-type]
            quotes,
            [(0, len(quotes))],
            REGEX_DICT.get(file_type, ALL_RFC_LIST_ENTRY_REGEX)
            # We can ignore this because we define REGEX ourselves in identifiers.
            # Which would be subject to user input.
        )
        for kwarg in req_kwargs:
            content: Optional[str] = kwarg.get("content")
            if content is not None:
                kwarg.setdefault("uri", "$".join([section.uri, content]))
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

# //= compliance/duvet-specification.txt#2.2.2
# //# List elements MAY contain a period (.) or exclamation point (!)
# //# and this punctuation MUST NOT terminate the requirement by excluding the following
# //# elements from the list of requirements.

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
