# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Public data structures for Duvet"""
from attrs import define, field

from duvet.identifiers import (
    AnnotationType,
    RequirementLevel,
    RequirementStatus,
    attested_type,
    implemented_type,
    omitted_type,
)


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
    :param bool omitted: A label with requirement marked true when there is exception for this requirement
    :param bool attested: A label with requirement marked true when there is annotation considered attested
    :param str content: Content of the requirement parsed from specification
    :param str id: A combination of the section id and content (Primary Key)(Foreign Key)
    :param dict matched_annotations: A hashtable of annotations matched with the requirement content and section id
    """

    requirement_level: RequirementLevel
    status: RequirementStatus = field(init=False, default=RequirementStatus.NOT_STARTED)
    implemented: bool = field(init=False, default=False)
    attested: bool = field(init=False, default=False)
    omitted: bool = field(init=False, default=False)
    content: str = ""
    id: str = ""
    matched_annotations: dict = field(init=False, default={})

    def __attrs_post_init__(self):
        """There MUST be a method that sets the status based on the labels.

        * Complete - The requirement MUST have both the labels Implemented and Attested
        * Missing Test - The requirement MUST only have the label Implemented
        * Exception - The requirement MUST only have the label Omitted
        * Missing Implementation - The requirement MUST only have the label Attested
        * Not started - The requirement MUST only have no labels at all.

        """
        self.set_labels()
        self.set_status()

    def set_status(self):
        """There MUST be a method that sets the status based on the labels."""
        if not self.omitted:
            if self.implemented:
                if self.attested:
                    self.status = RequirementStatus.COMPLETE
                else:
                    self.status = RequirementStatus.MISSING_TEST
            else:
                if self.attested:
                    self.status = RequirementStatus.MISSING_IMPLEMENTATION
                else:
                    self.status = RequirementStatus.NOT_STARTED

    def set_labels(self):
        """There MUST be a method that sets the labels based on matched_annotations.

        Implemented

        A specification requirement MUST be labeled implemented if there exists at least one matching annotation of type:

        * citation
        * untestable
        * deviation
        * implication

        Attested

        A specification requirement MUST be labeled attested if there exists at least one matching annotation of type

        * test
        * untestable
        * implication

        Omitted
        A specification requirement MUST be labeled omitted and MUST only be labeled omitted if there exists a matching annotation of type
        * exception

        """
        for anno in self.matched_annotations.values():
            if anno.type in implemented_type:
                self.implemented = True
            if anno.type in attested_type:
                self.attested = True
            if anno.type in omitted_type:
                self.omitted = True

    def add_annotation(self, anno):
        """There MUST be a method to add annotations."""
        new_dict = {anno.id: anno}
        self.matched_annotations.update(new_dict)
        if anno.type in implemented_type:
            self.implemented = True
        if anno.type in attested_type:
            self.attested = True
        if anno.type in omitted_type:
            self.omitted = True
        self.set_status()


@define
class Section:
    """
    The specification section shows the specific specification text and how this links to annotation.
    It MUST show all text from the section. It MUST highlight the text for every requirement. It MUST highlight the text that matches any annotation. Any highlighted text MUST have a mouse over that shows its annotation information.
    Clicking on any highlighted text MUST bring up a popup that shows


    :param  str id: a unique identifier of the section, for mark down documents it would be h1.h2.h3.h4 (Primary Key)
    :param  str title: the name of the title which we can target here using GitHub hyper link
    :param  int start_line: the line number of the start of the section
    :param  int end_line: the line number of the end of the section
    :param  dict requirements: a hashmap of requirements extracted from the section
    :param  bool has_requirements: a flag marked true when the length of the requirements field larger than 0, other wise it is false

    """
    title: str = field(init=False, default="")
    id: str = ""
    start_line: int = -1
    end_line: int = -1
    has_requirements: bool = field(init=False, default=False)
    requirements: dict = field(init=False, default={})

    def __attrs_post_init__(self):
        h = self.id.split(".")
        self.title = h[len(h) - 1]

    def add_requirement(self, requirement):
        new_dict = {requirement.id: requirement}
        self.has_requirements = True
        self.requirements.update(new_dict)
