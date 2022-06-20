# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Annotation Parser used by duvet-python."""
import logging
import pathlib
import re
from typing import List, Optional

import attr
from attrs import define, field

from duvet._config import DEFAULT_CONTENT_STYLE, DEFAULT_META_STYLE
from duvet.identifiers import AnnotationType
from duvet.structures import Annotation

__all__ = ["AnnotationParser"]

# //= compliance/duvet-specification.txt#2.3.1
# //= type=implication
# //# If a second meta line exists it MUST start with "type=".
logging.basicConfig(filename="annotation_parser.log", encoding="utf-8", level=logging.DEBUG)


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
    anno_type_regex: re.Pattern = field(init=False, default=re.compile(meta_style + r"[\s]type=" + r"(.*?)\n"))
    anno_meta_regex: re.Pattern = field(init=False, default=re.compile(meta_style + r"[\s](.*?)\n"))
    anno_content_regex: re.Pattern = field(init=False, default=re.compile(content_style + r"\s(.*?)\n"))

    def extract_implementation_file_annotations(self) -> List[Annotation]:
        """Given paths to implementation code, extract annotations.

        Return a list of annotation objects.
        """
        for filename in self.paths:  # pylint: disable=not-an-iterable
            temp_list = self._extract_file_annotations(filename)
            if len(temp_list) == 0:
                logging.info(
                    str(filename.resolve()) + "do not have any annotations. " "Skipping file"  # pylint: disable=w1201
                )
            self.annotations.extend(temp_list)
        return self.annotations

    def _extract_file_annotations(self, file_path: pathlib.Path) -> List[Annotation]:
        """Given a path of a implementation code.

        Return a list of annotation objects.
        """
        temp_annotation_list = []
        with open(file_path, "r", encoding="utf-8") as implementation_file:
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
                    temp_annotation_list.append(
                        self._extract_annotation_block(lines, annotation_start, annotation_end + 1, file_path)
                    )
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
                temp_annotation_list.append(
                    self._extract_annotation_block(lines, annotation_start, annotation_end + 1, file_path)
                )
                state = "CODE"
                annotation_start = -1
                annotation_end = -1
            curr_line += 1
            # Add edge case when annotation is at the end of the file.
            if annotation_start != -1 and annotation_end == len(lines) - 1:
                temp_annotation_list.append(
                    self._extract_annotation_block(lines, annotation_start, annotation_end + 1, file_path)
                )
        return temp_annotation_list

    def _extract_annotation_block(
        self, lines: List[str], annotation_start: int, annotation_end: int, file_path: pathlib.Path
    ) -> Optional[Annotation]:
        """Take a block of comments and extract one or none annotation object from it."""

        assert (
            annotation_start <= annotation_end
        ), f"Start must be less than or equal end. {annotation_start} !< {annotation_end}"
        new_lines = " ".join(lines[annotation_start:annotation_end])
        if not new_lines.endswith("\n"):
            new_lines = new_lines + "\n"
        return self._extract_annotation(new_lines, annotation_start, annotation_start, file_path)

    def _extract_annotation(self, lines: str, start: int, end: int, file_path: pathlib.Path) -> Optional[Annotation]:
        """Take a chunk of comments and extract or none annotation object from it."""

        # TODO: If temp_type is none. It could only be citation. # pylint: disable=fixme
        #   I will make another PR to address citation and exception.
        temp_type = re.search(self.anno_type_regex, lines)
        if temp_type is None:
            anno_type = AnnotationType["CITATION"]
        else:
            temp_type = temp_type.group(1).upper()
            anno_type = AnnotationType[temp_type]
        anno_content = ""
        target_meta = re.search(self.anno_meta_regex, lines)
        if target_meta is None:
            logging.warning(str(file_path.resolve()) + " Invalid annotation ")  # pylint: disable=w1201
            return None
        else:
            target_meta = target_meta.group(1)
        if re.findall(self.anno_content_regex, lines) is not None:
            for temp_content in re.findall(self.anno_content_regex, lines):
                anno_content = "".join([anno_content, temp_content])
        anno_content = anno_content.replace("\n", "")
        return Annotation(
            target_meta, anno_type, anno_content, start, end, "$".join([target_meta, anno_content]), file_path.resolve()
        )
