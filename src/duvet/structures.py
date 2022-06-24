# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Public data structures for Duvet."""
import logging

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

_LOGGER = logging.getLogger(__name__)


# noinspection PyUnresolvedReferences
@define
class Annotation:
    """Annotations are references to a text from a section in a specification.

    :param str target: Location of the section (Foreign Key)
    :param AnnotationType anno_type: An enumeration type of annotation
    :param str content: A string of the exact requirement words
    :param int start_line: Number of the start line of the annotation
    :param int end_line: Number of the end line of the annotation
    :param str location: Path to implementation file containing the annotation
    """

    target: str
    anno_type: AnnotationType
    content: str
    start_line: int
    end_line: int
    uri: str
    location: str

    def location_to_string(self) -> str:
        """Return annotation location."""
        return f"{self.location}#L{self.start_line}-L{self.end_line}"


# noinspection PyUnresolvedReferences
@define
class Requirement:
    """Any complete sentence containing at least one RFC 2119 keyword MUST be treated as a requirement.

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
    :param str uri: A combination of the section uri and content (Primary Key)(Foreign Key)
    :param dict matched_annotations: A hashtable of annotations matched with the requirement content and section uri
    """

    requirement_level: RequirementLevel
    status: RequirementStatus = field(init=False, default=RequirementStatus.NOT_STARTED)
    implemented: bool = field(init=False, default=False)
    attested: bool = field(init=False, default=False)
    omitted: bool = field(init=False, default=False)
    content: str = ""
    uri: str = ""
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
            if anno.anno_type in implemented_type:
                self.implemented = True
            if anno.anno_type in attested_type:
                self.attested = True
            if anno.anno_type in omitted_type:
                self.omitted = True

    def add_annotation(self, anno) -> bool:
        """There MUST be a method to add annotations."""
        new_dict = {anno.uri: anno}
        self.matched_annotations.update(new_dict)
        if anno.anno_type in implemented_type:
            self.implemented = True
        if anno.anno_type in attested_type:
            self.attested = True
        if anno.anno_type in omitted_type:
            self.omitted = True
        self.set_status()
        return True


# noinspection PyUnresolvedReferences
@define
class Section:
    """The specification section shows the specific specification text and how this links to annotation.

    It MUST show all text from the section. It MUST highlight the text for every requirement.
    It MUST highlight the text that matches any annotation.
    Any highlighted text MUST have a mouse over that shows its annotation information.
    Clicking on any highlighted text MUST bring up a popup that shows

    :param  str uri: a unique identifier of the section, for mark down documents it would be h1.h2.h3.h4 (Primary Key)
    :param  str title: the name of the title which we can target here using GitHub hyper link
    :param  int start_line: the line number of the start of the section
    :param  int end_line: the line number of the end of the section
    :param  dict requirements: a hashmap of requirements extracted from the section
    :param  bool has_requirements: a flag marked true when the length of the requirements field larger than 0
    """

    title: str = ""
    uri: str = ""
    start_line: int = -1
    end_line: int = -1
    has_requirements: bool = field(init=False, default=False)
    requirements: dict = field(init=False, default=attr.Factory(dict))

    def add_requirement(self, requirement: Requirement):
        """Add requirement to Section."""
        new_dict = {requirement.uri: requirement}
        self.has_requirements = True
        self.requirements.update(new_dict)

    def to_github_url(self, spec_dir, spec_github_url, branch_or_commit="master"):
        """URL for Section on GitHub."""
        header = self.uri.split(".")
        target_title = spec_dir + "#" + header[len(header) - 1]
        return "/".join([spec_github_url, "blob", branch_or_commit, target_title])

    def add_annotation(self, anno: Annotation) -> bool:
        """Add annotation to Section."""
        if anno.uri not in self.requirements.keys():
            _LOGGER.warning("%s not Found in %s", anno.uri, self.uri)
            return False
        else:
            return self.requirements[anno.uri].add_annotation(anno)  # pylint: disable=E1136


# noinspection PyUnresolvedReferences
@define
class Specification:
    """A specification is a document that defines correct behavior.

    A specification class is what we parsed from the specification document.
    Each specification contains multiple sections.

    :param str title: a string of the title of the specification
    :param str spec_dir: a relative path to the specification file (Primary Key)
    :param dict sections: a hash map of sections with the section.uri as the key and the section object as its value
    """

    title: str = ""
    spec_dir: str = ""
    sections: dict = field(init=False, default=attr.Factory(dict))  # hashmap equivalent in python

    def to_github_url(self, spec_github_url, branch_or_commit="master") -> str:
        """URL for Specification on GitHub."""
        return "/".join([spec_github_url, "blob", branch_or_commit, self.spec_dir])

    def add_section(self, section: Section):
        """Add Section to Specification."""
        new_dict = {section.uri: section}
        self.sections.update(new_dict)

    def add_annotation(self, annotation: Annotation) -> bool:
        """Add Annotation to Specification."""
        sec_id = annotation.target.split("#")[1]
        if sec_id not in self.sections.keys():
            _LOGGER.warning("%s not found in %s", annotation.target, self.spec_dir)
            return False
        else:
            return self.sections[sec_id].add_annotation(annotation)  # pylint: disable=E1136


# noinspection PyUnresolvedReferences
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
        """Add Specification to Report."""
        new_dict = {specification.spec_dir: specification}
        self.specifications.update(new_dict)

    def add_annotation(self, annotation: Annotation) -> bool:
        """Add Annotation to Report."""
        spec_id = annotation.target.split("#")[0]
        if spec_id not in self.specifications.keys():
            _LOGGER.warning("%s not found in report", spec_id)
            return False
        else:
            return self.specifications[spec_id].add_annotation(annotation)  # pylint: disable=E1136


@define
class ExceptionAnnotation(Annotation):
    """Exception annotations in duvet."""

    reason: str = field(init=False)
    has_reason: bool = field(init=False, default=False)

    def add_reason(self, reason: str):
        """Add reason to exception."""
        self.reason = reason
        self.has_reason = True
