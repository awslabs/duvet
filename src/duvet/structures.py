# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Public data structures for Duvet."""
import logging
from typing import Dict, Optional

import attr
from attrs import define, field

from duvet.identifiers import (
    ATTESTED_TYPES,
    EXCEPTED_TYPES,
    IMPLEMENTED_TYPES,
    AnnotationType,
    RequirementLevel,
    RequirementStatus,
)

_LOGGER = logging.getLogger(__name__)


@define
class Annotation:
    """Annotations are references to a text from a section in a specification.

    :param str target: Location of the section (Foreign Key)
    :param AnnotationType type: An enumeration type of annotation
    :param str content: A string of the exact requirement words
    :param int start_line: Number of the start line of the annotation
    :param int end_line: Number of the end line of the annotation
    :param str source: Path to implementation file containing the annotation
    """

    target: str
    type: AnnotationType
    content: str
    start_line: int
    end_line: int
    uri: str
    source: str
    reason: Optional[str] = field(init=True, default=None)

    def location_to_string(self) -> str:
        """Return annotation location."""
        return f"{self.source}#L{self.start_line}-L{self.end_line}"

    def has_reason(self):
        """Return True if there is a reason."""
        return self.reason is not None


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
    :param list matched_annotations: A hashtable of annotations matched with the requirement content and section uri
    """

    requirement_level: RequirementLevel
    status: RequirementStatus = field(init=False, default=RequirementStatus.NOT_STARTED)
    implemented: bool = field(init=False, default=False)
    attested: bool = field(init=False, default=False)
    excused: bool = field(init=False, default=False)
    unexcused: bool = field(init=False, default=False)

    content: str = ""
    uri: str = ""
    matched_annotations: list[Annotation] = field(init=False, default=attr.Factory(list))

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
        # //= compliance/duvet-specification.txt#2.6.1
        # //# The Requirement Statuses MUST be:
        labels = [self.implemented, self.attested, self.excused, self.unexcused]
        if labels == [True, False, False, False]:
            # //= compliance/duvet-specification.txt#2.6.1
            # //# *  Missing Proof - The requirement MUST only have the label "Implemented"
            self.status = RequirementStatus.MISSING_PROOF
        elif labels == [True, True, False, False]:
            # //= compliance/duvet-specification.txt#2.6.1
            # //# *  Complete - The requirement MUST have both the labels "Implemented" and "Attested"
            self.status = RequirementStatus.COMPLETE
        elif labels == [False, False, True, False]:
            # //= compliance/duvet-specification.txt#2.6.1
            # //# *  Excused - The requirement MUST only have the label "Excused"
            self.status = RequirementStatus.EXCUSED
        elif labels == [False, False, False, True]:
            # //= compliance/duvet-specification.txt#2.6.1
            # //# *  Missing Reason - The requirement MUST have the label "Unexcused"
            self.status = RequirementStatus.MISSING_REASON
        elif labels == [False, False, False, False]:
            # //= compliance/duvet-specification.txt#2.6.1
            # //# *  Not started - The requirement MUST NOT have any labels
            self.status = RequirementStatus.NOT_STARTED
        else:
            # //= compliance/duvet-specification.txt#2.6.1
            # //# *  Missing Implementation - The requirement MUST only have the label "Attested"
            self.status = RequirementStatus.DUVET_ERROR

    def set_labels(self):
        """There MUST be a method that sets the labels based on matched_annotations."""
        for annotation in self.matched_annotations:
            if annotation.type in IMPLEMENTED_TYPES:
                self.implemented = True
            if annotation.type in ATTESTED_TYPES:
                self.attested = True
            if annotation.type in EXCEPTED_TYPES:
                if annotation.has_reason():
                    self.excused = True
                else:
                    self.unexcused = True

    def add_annotation(self, annotation: Annotation) -> bool:
        """There MUST be a method to add annotations."""
        self.matched_annotations.append(annotation)
        return True

    def analyze_annotations(self) -> bool:
        """There MUST be a method to analyze annotations."""
        self.set_labels()
        self.set_status()
        return self.status in [RequirementStatus.COMPLETE, RequirementStatus.EXCUSED]


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
    """

    title: str = ""
    uri: str = ""
    start_line: int = -1
    end_line: int = -1
    has_requirements: bool = field(init=False, default=False)
    requirements: dict = field(init=False, default=attr.Factory(dict))
    lines: list = field(default=attr.Factory(list))

    def add_requirement(self, requirement: Requirement):
        """Add requirement to Section."""
        self.has_requirements = True
        if requirement.uri not in self.requirements.keys():
            new_dict = {requirement.uri: requirement}
            self.requirements.update(new_dict)

    def to_github_url(self, spec_dir, spec_github_url, branch_or_commit="master"):
        """URL for Section on GitHub."""
        section_full_title = self.uri.rsplit("#", 1)[-1]
        header = section_full_title.rsplit(".", 1)[-1]
        target_title = spec_dir + "#" + header
        return "/".join([spec_github_url, "blob", branch_or_commit, target_title])

    def add_annotation(self, annotation: Annotation) -> bool:
        """Add annotation to Section."""

        if annotation.uri in self.requirements.keys():
            return self.requirements[annotation.uri].add_annotation(annotation)

        if self._white_space_stripped_match(annotation):
            return True

        if self._substring_match(annotation):
            return True

        _LOGGER.warning("%s not found in %s", annotation.uri, self.uri)
        return False

    def analyze_annotations(self) -> bool:
        """Analyze report and return true if all MUST be marked complete."""
        return all(req.analyze_annotations() for req in self.requirements.values())

    def _white_space_stripped_match(self, annotation: Annotation) -> bool:

        # Compare by splitting space to list.
        for key in list(self.requirements.keys()):
            if str(key).split() == annotation.uri.split():
                return self.requirements[key].add_annotation(annotation)

        # Compare by getting rid of all space
        for key in self.requirements.keys():
            temp_key = "".join(str(key).split())
            temp_uri = "".join(annotation.uri.split())
            if temp_key == temp_uri:
                return self.requirements[key].add_annotation(annotation)

        return False

    def _substring_match(self, annotation: Annotation) -> bool:

        # Compare by splitting space to list.
        for key in list(self.requirements.keys()):
            if str(key).find(annotation.uri) != -1 or annotation.uri.find(key) != -1:
                return self.requirements[key].add_annotation(annotation)

        # Compare by getting rid of all space
        for key in list(self.requirements.keys()):
            temp_key = "".join(str(key).split())
            temp_uri = "".join(annotation.uri.split())
            if temp_key.find(temp_uri) != -1 or temp_uri.find(temp_key) != -1:
                return self.requirements[key].add_annotation(annotation)

        return False


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
    source: str = ""
    sections: dict = field(init=False, default=attr.Factory(dict))  # hashmap equivalent in python

    def to_github_url(self, spec_github_url, branch_or_commit="master") -> str:
        """URL for Specification on GitHub."""
        return "/".join([spec_github_url, "blob", branch_or_commit, self.source])

    def add_section(self, section: Section):
        """Add Section to Specification."""
        new_dict = {section.uri: section}
        self.sections.update(new_dict)

    def add_annotation(self, annotation: Annotation) -> bool:
        """Add Annotation to Specification."""
        section_uri = annotation.target
        if section_uri not in self.sections.keys():
            _LOGGER.warning("%s not found in %s", annotation.target, self.source)
            return False
        else:
            return self.sections[section_uri].add_annotation(annotation)

    def analyze_annotations(self) -> bool:
        """Analyze report and return true if all MUST marked complete."""
        specification_pass = True
        for section in self.sections.values():
            specification_pass = specification_pass and section.analyze_annotations()
        return specification_pass


@define
class Report:
    """Duvet's report shows how your project conforms to specifications.

    This lets you bound the correctness of your project.
    As you annotate the code in your project Duvet's report creates links between the implementation,
    the specification, and attestations.

    Duvet’s report aids customers in annotating their code.

    :param bool report_pass: A flag of pass or fail of this run, True for pass and False for fail
    :param dict specifications: a hashmap of specifications with specification directory as a key and
    specification object as a value
    """

    report_pass: bool = field(init=False, default=False)
    specifications: Dict[str, Specification] = field(init=False, default=attr.Factory(dict))

    def add_specification(self, specification: Specification):
        """Add Specification to Report."""
        new_dict = {specification.source: specification}
        self.specifications.update(new_dict)

    def add_annotation(self, annotation: Annotation) -> bool:
        """Add Annotation to Report."""
        specification_uri = annotation.target.split("#")[0]

        if specification_uri not in self.specifications.keys():
            _LOGGER.warning("%s not found in report", specification_uri)
            return False
        else:
            return self.specifications[specification_uri].add_annotation(annotation)

    def analyze_annotations(self) -> bool:
        """Analyze report."""
        self.report_pass = True
        for specification in self.specifications.values():
            self.report_pass = self.report_pass and specification.analyze_annotations()

        return self.report_pass


# //= compliance/duvet-specification.txt#2.2.4.1
# //= type=TODO
# //# Duvet SHOULD be able to record parsed requirements into Toml Files.

# //= compliance/duvet-specification.txt#2.2.1
# //= type=implication
# //# The name of the sections MUST NOT be nested.

# //= compliance/duvet-specification.txt#2.6.1
# //# Duvet MUST analyze the matching labels for every requirement; the result of this analysis
# //# is the requirement's Status.

# //= compliance/duvet-specification.txt#2.6.1
# //= type=implication
# //# Requirement Statuses MUST be exclusive.
