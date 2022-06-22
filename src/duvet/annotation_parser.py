# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Annotation Parser used by duvet-python."""
import logging
import pathlib
import re
from typing import List, Optional, Union

import attr
from attr import define, field

from duvet._config import DEFAULT_CONTENT_STYLE, DEFAULT_META_STYLE
from duvet.identifiers import AnnotationType
from duvet.structures import Annotation, ExceptionAnnotation

__all__ = ["AnnotationParser"]


@define
class AnnotationBlock:
    """Container of annotation block.

    Extract to annotation object or none.
    """

    lines: List[str]
    start: int
    end: int
    file_path: pathlib.Path
    anno_type_regex: re.Pattern
    anno_reason_regex: re.Pattern
    anno_meta_regex: re.Pattern
    anno_content_regex: re.Pattern
    meta: str = field(init=False)
    content: str = field(init=False, default="")
    quotes: str = field(init=False)
    target_meta: Optional[str] = field(init=False)

    def __attrs_post_init__(self):
        assert self.start <= self.end, f"Start must be less than or equal end. {self.start} !< {self.end}"
        self.quotes = " ".join(self.lines[self.start: self.end])
        if not self.quotes.endswith("\n"):
            self.quotes = self.quotes + "\n"
        # Get content string in the annotation.
        if re.findall(self.anno_content_regex, self.quotes) is not None:
            for temp_content in re.findall(self.anno_content_regex, self.quotes):
                self.content = " ".join([self.content, temp_content])
        self.content = self.content.replace("\n", " ").strip()
        # Get meta string in the annotation.
        self.meta = self.quotes.replace(self.content, "")
        # Get target from meta string in the annotation.
        temp_target = re.search(self.anno_meta_regex, self.meta)
        if temp_target is None:
            logging.warning(str(self.file_path.resolve()) + " Invalid annotation ")  # pylint: disable=w1201
            self.target_meta = None
        else:
            self.target_meta = temp_target.group(1)

    def to_annotation(self) -> Union[Annotation, ExceptionAnnotation, None]:
        """Take a chunk of comments and extract or none annotation object from it."""

        return self._extract_annotation()

    def _to_exception(self) -> Optional[ExceptionAnnotation]:
        anno_reason = re.search(self.anno_reason_regex, self.meta)
        result = ExceptionAnnotation(
            self.target_meta,
            AnnotationType.EXCEPTION,
            self.content,
            self.start,
            self.end,
            "$".join([self.target_meta, self.content]),
            self.file_path.resolve(),
        )
        # Add reason to exception annotation if reason detected.
        if anno_reason is not None:
            anno_reason_str = anno_reason.group(1)
            if re.findall(self.anno_content_regex, self.meta[anno_reason.span()[1]:]) is not None:
                for temp_content in re.findall(self.anno_meta_regex, self.meta[anno_reason.span()[1]:]):
                    anno_reason_str = " ".join([anno_reason_str, temp_content])
            result.add_reason(anno_reason_str)
        return result

    def _extract_annotation(
            self,
    ) -> Union[Annotation, ExceptionAnnotation, None]:
        if self.target_meta is None:
            return None
        temp_type = re.search(self.anno_type_regex, self.quotes)
        if temp_type is None:
            anno_type = AnnotationType["CITATION"]
        else:
            temp_type = temp_type.group(1).upper()
            anno_type = AnnotationType[temp_type]
        if anno_type == AnnotationType.EXCEPTION:
            return self._to_exception()
        else:
            return Annotation(
                self.target_meta,
                anno_type,
                self.content,
                self.start,
                self.end,
                "$".join([self.target_meta, self.content]),
                self.file_path.resolve(),
            )


@define
class AnnotationParser:
    """Parser for annotation from implementation."""

    paths: List[pathlib.Path] = field(init=True, default=attr.Factory(list))
    annotations: List[Annotation] = field(init=False, default=attr.Factory(list))
    # //= compliance/duvet-specification.txt#2.3.1
    # //= type=implication
    # //# This identifier of meta parts MUST
    # //# be configurable.
    meta_style: str = DEFAULT_META_STYLE
    content_style: str = DEFAULT_CONTENT_STYLE

    # TODO: Sanitize user input for regular expression usage # pylint: disable=fixme

    def extract_implementation_file_annotations(self) -> List[Annotation]:
        """Given paths to implementation code, extract annotations.

        Return a list of annotation objects.
        """
        for filename in self.paths:  # pylint: disable=not-an-iterable
            # temp_list = self._extract_file_annotations(filename)
            temp_list = AnnotationFile(filename, self.meta_style, self.content_style).to_annotations()
            if len(temp_list) == 0:
                logging.info("%s does not have any annotations. Skipping.", str(filename.resolve()))
            self.annotations.extend(temp_list)
        return self.annotations


@define
class AnnotationFile:
    """Container of annotation file.

    Extract to annotation objects list.
    """

    file_path: pathlib.Path
    meta_style: str = field(init=True)
    content_style: str = field(init=True)
    anno_type_regex: re.Pattern = field(init=False)
    anno_reason_regex: re.Pattern = field(init=False)
    anno_meta_regex: re.Pattern = field(init=False)
    anno_content_regex: re.Pattern = field(init=False)
    annotations: List[Annotation] = field(init=False, default=attr.Factory(list))

    def __attrs_post_init__(self):
        # //= compliance/duvet-specification.txt#2.3.1
        # //= type=implication
        # //# If a second meta line exists it MUST start with "type=".
        self.anno_type_regex = re.compile(self.meta_style + r"[\s]type=" + r"(.*?)\n")
        self.anno_reason_regex = re.compile(self.meta_style + r"[\s]reason=" + r"(.*?)\n")
        self.anno_meta_regex = re.compile(self.meta_style + r"[\s](.*?)\n")
        self.anno_content_regex = re.compile(self.content_style + r"\s(.*?)\n")

    def _add_annotation(self, anno: Optional[Annotation]):
        if anno is not None:
            self.annotations.append(anno)

    def to_annotations(self) -> List[Annotation]:
        """Given a path of a implementation code.

        Return a list of annotation objects.
        """

        with open(self.file_path, "r", encoding="utf-8") as implementation_file:
            lines = implementation_file.readlines()
        curr_line = 0
        annotation_start = -1
        annotation_end = -1
        state = "CODE"
        while curr_line < len(lines):
            line = lines[curr_line]
            # If curr_line is part of anno_meta.
            if re.search(r"[\s]*" + self.meta_style, line) is not None:
                # Check current state. If state is ANNO_CONTENT.
                # We should let helper function create an annotation object.
                if state == "ANNO_CONTENT":
                    temp_anno_block = AnnotationBlock(
                        lines,
                        annotation_start,
                        annotation_end + 1,
                        self.file_path,
                        self.anno_type_regex,
                        self.anno_reason_regex,
                        self.anno_meta_regex,
                        self.anno_content_regex,
                    ).to_annotation()
                    self._add_annotation(temp_anno_block)
                    state = "ANNO_META"
                    annotation_start = curr_line
                    annotation_end = curr_line
                elif state == "CODE":
                    # It should be true if the function is doing it is supposed to do.
                    assert annotation_start == -1
                    state = "ANNO_META"
                    annotation_start = curr_line
                    annotation_end = curr_line
                elif state == "ANNO_META":
                    annotation_end = curr_line
            elif re.search(r"[\s]*" + self.content_style, line) is not None:
                state = "ANNO_CONTENT"
                if annotation_start == -1:
                    annotation_start = curr_line
                    annotation_end = curr_line
                else:
                    annotation_end = curr_line
            elif annotation_start != -1 and annotation_end != -1:
                temp_anno_block = AnnotationBlock(
                    lines,
                    annotation_start,
                    annotation_end + 1,
                    self.file_path,
                    self.anno_type_regex,
                    self.anno_reason_regex,
                    self.anno_meta_regex,
                    self.anno_content_regex,
                ).to_annotation()
                self._add_annotation(temp_anno_block)
                state = "CODE"
                annotation_start = -1
                annotation_end = -1
            curr_line += 1
            # Add edge case when annotation is at the end of the file.
            if annotation_start != -1 and annotation_end == len(lines) - 1:
                temp_anno_block = AnnotationBlock(
                    lines,
                    annotation_start,
                    annotation_end + 1,
                    self.file_path,
                    self.anno_type_regex,
                    self.anno_reason_regex,
                    self.anno_meta_regex,
                    self.anno_content_regex,
                ).to_annotation()
                self._add_annotation(temp_anno_block)
        return self.annotations
