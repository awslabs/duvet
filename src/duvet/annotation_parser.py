# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Annotation Parser used by duvet-python."""
import pathlib
import re
from typing import List

from attr import field
from attrs import define

from duvet._config import DEFAULT_CONTENT_STYLE, DEFAULT_META_STYLE
from duvet.identifiers import AnnotationType
from duvet.structures import Annotation

__all__ = ["AnnotationParser"]

ANNO_META_TYPE_REGEX = r"^(//=\stype=)"
# Match A List identifier
IS_ANNO_META_TYPE_REGEX = re.compile(ANNO_META_TYPE_REGEX)
# Match All List identifiers
ALL_ANNO_META_TYPE_REGEX = re.compile(ANNO_META_TYPE_REGEX, re.MULTILINE)

ANNO_META_TARGET_REGEX = r"^(//=\s)"
# Match A List identifier
IS_ANNO_META_TARGET_REGEX = re.compile(ANNO_META_TYPE_REGEX)
# Match All List identifiers
ALL_ANNO_META_TARGET_REGEX = re.compile(ANNO_META_TYPE_REGEX, re.MULTILINE)

TEST_DAFNY_STR = (
    "//= compliance/client-apis/client.txt#2.4.2.1\n"
    "//= type=implication\n"
    "//# * encrypt (encrypt.md) MUST only support algorithm suites that have\n"
    "//# a Key Commitment (../framework/algorithm-suites.md#algorithm-\n"
    "//# suites-encryption-key-derivation-settings) value of False\n"
)


@define
class AnnotationParser:
    """Parser for annotation from implementation."""

    paths: [pathlib.Path] = field(init=True)
    annotations: List[Annotation] = field(init=False, default=[])
    meta_style: str = DEFAULT_META_STYLE
    content_style: str = DEFAULT_CONTENT_STYLE

    def extract_file_annotations(self, file_path: pathlib.Path) -> List[Annotation]:
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
            if line.startswith(self.meta_style) or line.startswith(self.meta_style):
                if annotation_start == -1:
                    annotation_start = curr_line
                    annotation_end = curr_line
                else:
                    annotation_end = curr_line
            else:
                temp_annotation_list.append(
                    self._extract_annotation_block(lines, annotation_start, annotation_end, file_path)
                )
                annotation_start = -1
                annotation_end = -1
            curr_line += 1
        return temp_annotation_list

    def _extract_annotation_block(
            self, lines: List[str], annotation_start: int, annotation_end: int, file_path: pathlib.Path
    ) -> Annotation:
        new_lines = "".join(lines[annotation_start:annotation_end])
        return self._extract_annotation(new_lines, annotation_start, annotation_start, file_path)

    def _extract_annotation(self, lines: str, start: int, end: int, file_path: pathlib.Path) -> Annotation:
        """Take a chunk of comments and extract annotation object from it.

        TODO: This part needed to be configurable by customer.
        We will implement it in the future.
        We will not support it for now.
        """

        anno_type_regex = re.compile(self.meta_style + r"[\s]type=" + r"(.*?)\n")
        temp_type = re.search(anno_type_regex, lines).group(1).upper()
        # temp_type = re.search(r'//=\stype=(.*?)\n', lines).group(1).upper()
        anno_type = AnnotationType[temp_type]
        anno_content = ""
        target_meta = re.search(r"//=\s(.*?)\n", lines).group(1)
        if re.findall(r"//#\s(.*?)\n", lines) is not None:
            for temp_content in re.findall(r"//#\s(.*?)\n", lines):
                anno_content = "".join([anno_content, temp_content])
        anno_content = anno_content.replace("\n", "")
        return Annotation(
            target_meta, anno_type, anno_content, start, end, "$".join([target_meta, anno_content]), file_path.resolve()
        )
