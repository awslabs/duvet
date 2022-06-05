# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Public data structures for Duvet."""
import warnings

import attr
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
    """Annotations are references to a text from a section in a specification.

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
    matched_annotations: dict = field(init=False, default=attr.Factory(dict))

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

        A specification requirement MUST be labeled implemented
        if there exists at least one matching annotation of type:

        * citation
        * untestable
        * deviation
        * implication

        Attested

        A specification requirement MUST be labeled attested
         if there exists at least one matching annotation of type

        * test
        * untestable
        * implication

        Omitted
        A specification requirement MUST be labeled omitted and
        MUST only be labeled omitted if there exists a matching annotation of type
        * exception
        """
        for anno in self.matched_annotations.values():
            if anno.type in implemented_type:
                self.implemented = True
            if anno.type in attested_type:
                self.attested = True
            if anno.type in omitted_type:
                self.omitted = True

    def add_annotation(self, anno) -> bool:
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
        return True


@define
class Section:
    """
    The specification section shows the specific specification text and how this links to annotation.
    It MUST show all text from the section. It MUST highlight the text for every requirement.
    It MUST highlight the text that matches any annotation.
    Any highlighted text MUST have a mouse over that shows its annotation information.
    Clicking on any highlighted text MUST bring up a popup that shows


    :param  str id: a unique identifier of the section, for mark down documents it would be h1.h2.h3.h4 (Primary Key)
    :param  str title: the name of the title which we can target here using GitHub hyper link
    :param  int start_line: the line number of the start of the section
    :param  int end_line: the line number of the end of the section
    :param  dict requirements: a hashmap of requirements extracted from the section
    :param  bool has_requirements: a flag marked true when the length of the requirements field larger than 0, other wise it is false
    """

    title: str = ""
    id: str = ""
    start_line: int = -1
    end_line: int = -1
    has_requirements: bool = field(init=False, default=False)
    requirements: dict = field(init=False, default=attr.Factory(dict))

    def add_requirement(self, requirement: Requirement):
        new_dict = {requirement.id: requirement}
        self.has_requirements = True
        self.requirements.update(new_dict)

    def to_github_url(self, spec_dir, spec_github_url, branch_or_commit="master"):
        h = self.id.split(".")
        target_title = spec_dir + "#" + h[len(h) - 1]
        return "/".join([spec_github_url, "blob", branch_or_commit, target_title])

    def add_annotation(self, anno: Annotation) -> bool:
        if anno.id not in self.requirements.keys():
            print(anno.id + " not Found in " + self.id)
            return False
        else:
            return self.requirements[anno.id].add_annotation(anno)


@define
class Specification:
    """
    A specification is a document, like this, that defines correct behavior. This behavior is defined in regular human language.
    A specification class is what we parsed from the specification document. Each specification contains multiple sections

    :param str title: a string of the title of the specification
    :param str spec_dir: a relative path to the specification file (Primary Key)
    :param dict sections: a hash map of sections with the section.id as the key and the section object as its value
    """

    title: str = ""
    spec_dir: str = ""
    sections: dict = field(init=False, default=attr.Factory(dict))  # hashmap equivalent in python

    def to_github_url(self, spec_github_url, branch_or_commit="master") -> str:
        return "/".join([spec_github_url, "blob", branch_or_commit, self.spec_dir])

    def add_section(self, section: Section):
        new_dict = {section.id: section}
        self.sections.update(new_dict)

    def add_annotation(self, annotation: Annotation) -> bool:
        sec_id = annotation.target.split("#")[1]
        if sec_id not in self.sections.keys():
            print(annotation.target + " not found in specification")
            return False
        else:
            return self.sections[sec_id].add_annotation(annotation)


@define
class Report:
    """Duvet's report shows how your project conforms to specifications.

    This lets you bound the correctness of your project.
    As you annotate the code in your project Duvet's report creates links between the implementation,
    the specification, and attestations.

    Duvet’s report aids customers in annotating their code.

    :param bool pass_fail: A flag of pass or fail of this run, True for pass and False for fail
    :param dict specifications: a hashmap of specifications with specification directory as a key and
    specification object as a value
    """

    pass_fail: bool = field(init=False, default=False)
    specifications: dict = field(init=False, default=attr.Factory(dict))

    def add_specification(self, specification: Specification):
        new_dict = {specification.spec_dir: specification}
        self.specifications.update(new_dict)

    def add_annotation(self, annotation: Annotation) -> bool:
        spec_id = annotation.target.split("#")[0]
        if spec_id not in self.specifications.keys():
            print(spec_id + " not found in report")
            return False
        else:
            return self.specifications[spec_id].add_annotation(annotation)
