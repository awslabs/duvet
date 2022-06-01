# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Public data structures for Duvet"""
from attrs import define

from duvet.identifiers import AnnotationType, RequirementLevel


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
