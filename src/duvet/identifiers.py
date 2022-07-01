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


IMPLEMENTED_TYPE = [
    AnnotationType.CITATION,
    AnnotationType.UNTESTABLE,
    AnnotationType.DEVIATION,
    AnnotationType.IMPLICATION,
]
ATTESTED_TYPE = [AnnotationType.TEST, AnnotationType.UNTESTABLE, AnnotationType.IMPLICATION]
EXCEPTED_TYPE = [AnnotationType.EXCEPTION]
