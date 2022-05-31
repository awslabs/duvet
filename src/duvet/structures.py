"""Public data structures for Duvet"""
from attrs import define
from enum import Enum


class AnnotationType(Enum):
    CITATION = 1
    TEST = 2
    UNTESTABLE = 3
    DEVIATION = 4
    EXCEPTION = 5
    IMPLICATION = 6
    TODO = 7


class AnnotationLevel(Enum):
    MUST = 1
    SHOULD = 2
    MAY = 3


@define
class Annotation:
    """ An annotation class is what we parsed from the src/test code files
        :param str target: Location of the section (Foreign Key)
        :param AnnotationType type: An enumeration type of annotation
        :param str content: A string of the exact requirement words
        :param int start_line: Number of the start line
        :param int end_line: Number of the end line
        :param str location: A string of the location
        """
    target: str
    type: AnnotationType
    content: str
    start_line: int
    end_line: int
    id: str
    location: str