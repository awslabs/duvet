# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Public data structures for Duvet"""
from attrs import define

from duvet.identifiers import AnnotationType, RequirementLevel, RequirementStatus


@define
class Annotation:
    """Annotations are references to a text from a section in a specification,
    written as comment in the source code and test code.
    :param str target: Location of the section (Foreign Key)
    :param AnnotationType type: An enumeration type of annotation
    :param str content: A string of the exact requirement words
    :param int start_line: Number of the start line of the annotation
    :param int end_line: Number of the end line of the annotation
    :param str location: Path to implementation file containing the annotation
    """

    target: str
    type: AnnotationType
    content: str
    start_line: int
    end_line: int
    id: str
    location: str


@define
class Requirement:
    """
    Any complete sentence containing at least one RFC 2119 keyword MUST be treated as a requirement.
    A requirement MAY contain multiple RFC 2119 keywords. A requirement MUST be terminated by one of the following

    * period (.)
    * exclamation point (!)
    * list
    * table

    :param RequirementLevel requirement_level: Location of the section (Foreign Key)
    :param RequirementStatus status: An enumeration type of annotation
    :param bool implemented: A label with requirement marked true when there is annotation considered implemented
    :param bool omitted: A label with requirement marked true when there is annotation considered attested
    :param str content:  A label with requirement marked true when there is exception for this requirement
    :param str id: A combination of the section id and content (Primary Key)(Foreign Key)
    :param dict matched_annotations: A hashtable of annotations matched with the requirement content and section id
    """

    requirement_level: RequirementLevel
    status: RequirementStatus
    implemented: bool
    attested: bool
    omitted: bool
    content: str
    id: str
    matched_annotations: dict
