# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Annotation Parser used by duvet-python."""
import pathlib
import re
import warnings
from typing import List

import attr
from attrs import define, field

from duvet._config import DEFAULT_CONTENT_STYLE, DEFAULT_META_STYLE
from duvet.identifiers import AnnotationType
from duvet.structures import Annotation

__all__ = ["AnnotationParser"]


# //= compliance/duvet-specification.txt#2.3.1
# //= type=implication
# //# If a second meta line exists it MUST start with "type=".


@define
class AnnotationParser:
    """Parser for annotation from implementation."""

    paths: list[pathlib.Path] = field(init=True, default=attr.Factory(list))
    annotations: List[Annotation] = field(init=False, default=attr.Factory(list))
    # //= compliance/duvet-specification.txt#2.3.1
    # //= type=implication
    # //# This identifier of meta parts MUST
    # //# be configurable.
    meta_style: str = DEFAULT_META_STYLE
    content_style: str = DEFAULT_CONTENT_STYLE
    anno_type_regex: re.Pattern = field(init=False, default=re.compile(meta_style + r"[\s]type=" + r"(.*?)\n"))
    anno_meta_regex: re.Pattern = field(init=False, default=re.compile(meta_style + r"[\s](.*?)\n"))
    anno_content_regex: re.Pattern = field(init=False, default=re.compile(content_style + r"\s(.*?)\n"))

    def extract_implementation_file_annotations(self) -> List[Annotation]:
        """Given paths to  implementation code, extract annotations.

        Return a list of annotation objects.
        """
        for filename in self.paths:  # pylint: disable=not-an-iterable
            temp_list = self._extract_file_annotations(filename)
            if len(temp_list) == 0:
                warnings.warn(f"{str(filename.resolve())} do not have any annotations. Skipping file")
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
        while curr_line < len(lines):
            line = lines[curr_line]
            if (
                re.search(r"[\s]*" + self.meta_style, line) is not None
                or re.search(r"[\s]*" + self.content_style, line) is not None
            ):
                if annotation_start == -1:
                    annotation_start = curr_line
                    annotation_end = curr_line
                else:
                    annotation_end = curr_line
            elif annotation_start != -1 and annotation_end != -1:
                temp_annotation_list.append(
                    self._extract_annotation_block(lines, annotation_start, annotation_end + 1, file_path)
                )
                annotation_start = -1
                annotation_end = -1
            curr_line += 1
        return temp_annotation_list

    def _extract_annotation_block(
        self, lines: List[str], annotation_start: int, annotation_end: int, file_path: pathlib.Path
    ) -> Annotation:
        """Take a block of comments and extract one or none annotation object from it."""

        new_lines = "".join(lines[annotation_start:annotation_end])
        return self._extract_annotation(new_lines, annotation_start, annotation_start, file_path)

    def _extract_annotation(self, lines: str, start: int, end: int, file_path: pathlib.Path) -> Annotation:
        """Take a chunk of comments and extract or none annotation object from it."""

        temp_type = re.search(self.anno_type_regex, lines).group(1).upper()
        anno_type = AnnotationType[temp_type]
        anno_content = ""
        target_meta = re.search(self.anno_meta_regex, lines).group(1)
        if re.findall(self.anno_content_regex, lines) is not None:
            for temp_content in re.findall(self.anno_content_regex, lines):
                anno_content = "".join([anno_content, temp_content])
        anno_content = anno_content.replace("\n", "")
        return Annotation(
            target_meta, anno_type, anno_content, start, end, "$".join([target_meta, anno_content]), file_path.resolve()
        )
