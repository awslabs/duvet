# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Unique identifiers used by duvet-python."""
from enum import Enum

__version__ = "0.0.1"


class AnnotationType(Enum):
    """definition of type of annotation."""

    CITATION = 1
    TEST = 2
    UNTESTABLE = 3
    DEVIATION = 4
    EXCEPTION = 5
    IMPLICATION = 6
    TODO = 7


class RequirementLevel(Enum):
    """Static definition of level of requirement."""

    MUST = 1
    SHOULD = 2
    MAY = 3


class RequirementStatus(Enum):
    """Static definition of status of requirement."""

    COMPLETE = 1
    MISSING_PROOF = 2
    EXCUSED = 3
    MISSING_IMPLEMENTATION = 4
    NOT_STARTED = 5
    MISSING_REASON = 6
    DUVET_ERROR = 7


# //= compliance/duvet-specification.txt#2.5.1
# //# A specification requirement MUST be labeled "Implemented"
# //# if there exists at least one matching annotation of type:
IMPLEMENTED_TYPES = [
    AnnotationType.CITATION,
    AnnotationType.UNTESTABLE,
    AnnotationType.DEVIATION,
    AnnotationType.IMPLICATION,
]

# //= compliance/duvet-specification.txt#2.5.2
# //# A specification requirement MUST be labeled "Attested" if there exists at least one matching annotation of type

ATTESTED_TYPES = [AnnotationType.TEST, AnnotationType.UNTESTABLE, AnnotationType.IMPLICATION]
EXCEPTED_TYPES = [AnnotationType.EXCEPTION]

DEFAULT_HTML_PATH = "specification_compliance_report.html"
DEFAULT_JSON_PATH = "duvet-result.json"
