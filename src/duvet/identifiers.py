# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Duvet data structures for defining implementation-specific characteristics."""
from enum import Enum

"""Unique identifiers used by duvet-python."""
__all__ = ("__version__",)
__version__ = "0.0.1"


class AnnotationType(Enum):
    """definition of type of annotation.
    """
    CITATION = 1
    TEST = 2
    UNTESTABLE = 3
    DEVIATION = 4
    EXCEPTION = 5
    IMPLICATION = 6
    TODO = 7


class RequirementLevel(Enum):
    """Static definition of level of requirement.
    """
    MUST = 1
    SHOULD = 2
    MAY = 3
