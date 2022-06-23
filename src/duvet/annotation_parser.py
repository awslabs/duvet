# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Annotation Parser used by duvet-python."""
# pylint: disable=fixme, W1203
import logging
import pathlib
import re
from typing import List

import attr
from attrs import define, field

from duvet._config import DEFAULT_CONTENT_STYLE, DEFAULT_META_STYLE
from duvet.identifiers import AnnotationType
from duvet.structures import Annotation, ExceptionAnnotation

__all__ = ["AnnotationParser", "AnnotationBlock", "AnnotationFile"]


@define
class AnnotationParser:
    """Parser for annotation from implementation."""

    paths: list[pathlib.Path] = field(init=True, default=attr.Factory(list))
    annotations: list[Annotation] = field(init=False, default=attr.Factory(list))
    meta_style: str = DEFAULT_META_STYLE
    content_style: str = DEFAULT_CONTENT_STYLE


@define
class AnnotationFile:
    """Container of annotation file.

    Extract to annotation objects list.
    """

    file_path: pathlib.Path
    anno_parser: AnnotationParser = field(init=True)
    lines: list[str] = field(init=False, default=attr.Factory(list))
    annotations: list[Annotation] = field(init=False, default=attr.Factory(list))

    def __attrs_post_init__(self):
        """Parse lines form file."""

        with open(self.file_path, "r", encoding="utf-8") as implementation_file:
            self.lines = implementation_file.readlines()

    def get_meta_style(self):
        """Get meta style regex from parser."""

        return self.anno_parser.meta_style

    def get_content_style(self):
        """Get content style regex from parser."""

        return self.anno_parser.content_style


@define
class AnnotationBlock:
    """Container of annotation block.

    Extract to annotation object or none.
    """

    start: int
    end: int
    anno_file: AnnotationFile = field(init=True)
    anno_type_regex: re.Pattern = field(init=False)
    anno_reason_regex: re.Pattern = field(init=False)
    anno_meta_regex: re.Pattern = field(init=False)
    anno_content_regex: re.Pattern = field(init=False)

    def __attrs_post_init__(self):
        """Create regular expression based on config."""

        self.anno_type_regex = re.compile(self.anno_file.get_meta_style() + r"[\s]type=" + r"(.*?)\n")
        self.anno_reason_regex = re.compile(self.anno_file.anno_parser.meta_style + r"[\s]reason=" + r"(.*?)\n")
        self.anno_meta_regex = re.compile(self.anno_file.get_meta_style() + r"[\s](.*?)\n")
        self.anno_content_regex = re.compile(self.anno_file.get_content_style() + r"\s(.*?)\n")
        assert self.start <= self.end, f"Start must be less than or equal end. {self.start} !< {self.end}"

        # TODO: add abstraction on this code by parsing attributes using reference
        #
        # TODO: add abstraction on mardown parser by adding logic on parser  instead of parser itself


def clean_content(content: str, anno_content_regex: re.Pattern) -> str:
    """Create clean content string."""

    if not content.endswith("\n"):
        content = content + "\n"
    cleaned_content = ""
    if re.findall(anno_content_regex, content) is not None:
        for temp_content in re.findall(anno_content_regex, content):
            cleaned_content = " ".join([cleaned_content, temp_content])
    cleaned_content = cleaned_content.replace("\n", " ").strip()
    return cleaned_content


def exceptions_from_block_helper(target_meta, content, meta, self: AnnotationBlock) -> List[ExceptionAnnotation]:
    """Given a block of annotation.

    Return a list of exception objects.
    """
    temp_list = []
    anno_reason = re.search(self.anno_reason_regex, meta)
    content = clean_content(content, self.anno_content_regex)
    result = ExceptionAnnotation(
        target_meta,
        AnnotationType.EXCEPTION,
        content,
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
    temp_list.append(result)
    return temp_list


def annotations_from_block_helper(quotes, content, meta, target_meta, block: AnnotationBlock) -> List[Annotation]:
    """Given a block of a annotation.

    Return a list of annotation objects.
    """

    temp_list = []
    temp_type = re.search(block.anno_type_regex, quotes)
    content = clean_content(content, block.anno_content_regex)
    if temp_type is None:
        anno_type = AnnotationType["CITATION"]
    else:
        temp_type = temp_type.group(1).upper()
        anno_type = AnnotationType[temp_type]
    if anno_type == AnnotationType.EXCEPTION:
        exception_list = exceptions_from_block_helper(target_meta, content, meta, block)
        temp_list.append(exception_list)
    else:
        temp_anno = Annotation(
            target_meta,
            anno_type,
            content,
            block.start,
            block.end,
            "$".join([target_meta, content]),
            block.anno_file.file_path.resolve(),
        )
        # print(temp_anno)
        temp_list.append(temp_anno)
    return temp_list


def annotations_from_block(block: AnnotationBlock) -> List[Annotation]:
    """Take a chunk of comments and extract or none annotation object from it."""

    def _check_target(block: AnnotationBlock, meta_str: str, content_str: str) -> List[Annotation]:
        temp_list = []
        temp_target = re.search(block.anno_meta_regex, meta_str)
        if temp_target is None:
            logging.warning(
                f"{str(block.anno_file.file_path.resolve())} "
                f"L{str(curr_line)} Invalid annotation "
            )
            return temp_list
        else:
            target_meta = temp_target.group(1)
            quotes = meta_str + content_str
            temp_annos = annotations_from_block_helper(quotes, content_str, meta_str, target_meta, block)
            temp_list.extend(temp_annos)
        return temp_list

    annotations = []
    curr_line = block.start
    meta_str = ""
    content_str = ""
    state = "START"
    while curr_line < block.end:
        meta_line = re.search(r"[\s]*" + block.anno_file.get_meta_style(), block.anno_file.lines[curr_line])
        content_line = re.search(r"[\s]*" + block.anno_file.get_content_style(), block.anno_file.lines[curr_line])
        if meta_line is not None:
            if state in ("START", "META"):
                # print(meta_line)
                meta_str = "".join([meta_str, block.anno_file.lines[curr_line]])
            elif state == "CONTENT":
                # annotations.extend(_check_target(block,meta_str,content_str))
                new_block = AnnotationBlock(curr_line, block.end, block.anno_file)
                # print(new_block)
                new_annos = annotations_from_block(new_block)
                # print(new_annos)
                block.end = curr_line
                annotations.extend(_check_target(block, meta_str, content_str))
                annotations.extend(new_annos)
                return annotations
            state = "META"
        elif content_line is not None:
            if state == "START":
                logging.warning(f"{str(block.anno_file.file_path.resolve())} L{str(curr_line)} Invalid annotation ")
                return annotations
            else:
                content_str = "".join([content_str, block.anno_file.lines[curr_line]])
                state = "CONTENT"
        curr_line += 1

    # print(block)
    annotations.extend(_check_target(block, meta_str, content_str))
    return annotations


def blocks_from_file(anno_file: AnnotationFile) -> List[AnnotationBlock]:
    """Given a path of a implementation code.

    Return a list of annotation blocks.
    """

    annotation_blocks: list[AnnotationBlock] = []

    curr_line = 0
    annotation_start = -1
    annotation_end = -1
    state = "CODE"
    while curr_line < len(anno_file.lines):
        # print(state)
        line = anno_file.lines[curr_line]
        # If curr_line is part of anno
        is_meta = re.search(r"[\s]*" + anno_file.anno_parser.meta_style, line) is not None
        is_content = re.search(r"[\s]*" + anno_file.anno_parser.content_style, line) is not None
        if is_meta or is_content:
            # Check current state. If state is ANNO
            # We should let helper function create an annotation object.
            if state == "ANNO_BLOCK":
                annotation_end = curr_line
            elif state == "CODE":
                # It should be true if the function is doing it is supposed to do.
                assert annotation_start == -1
                state = "ANNO_BLOCK"
                annotation_start = curr_line
                annotation_end = curr_line
        elif annotation_start != -1 and annotation_end != -1:
            temp_anno_block = AnnotationBlock(annotation_start, annotation_end + 1, anno_file)
            annotation_blocks.append(temp_anno_block)
            state = "CODE"
            annotation_start = -1
            annotation_end = -1
        curr_line += 1
    # Add edge case when annotation is at the end of the file.
    if annotation_start != -1 and annotation_end == len(anno_file.lines) - 1:
        temp_anno_block = AnnotationBlock(annotation_start, annotation_end + 1, anno_file)
        annotation_blocks.append(temp_anno_block)
    return annotation_blocks


def annotations_from_file(file: AnnotationFile) -> List[Annotation]:
    """Given an annotation file.

    Return a list of annotation objects.
    """
    for block in blocks_from_file(file):
        file.annotations.extend(annotations_from_block(block))
    return file.annotations


# TODO: Sanitize user input for regular expression usage
def annotations_from_parser(parser: AnnotationParser) -> List[Annotation]:
    """Given paths to implementation code, extract annotations.

    Return a list of annotation objects.
    """
    for filename in parser.paths:  # pylint: disable=not-an-iterable
        # temp_list = self._extract_file_annotations(filename)
        temp_list = annotations_from_file(AnnotationFile(filename, parser))
        if len(temp_list) == 0:
            logging.info("%s does not have any annotations. Skipping.", str(filename.resolve()))
        parser.annotations.extend(temp_list)
        # print(temp_list)
        # print(parser)
    return parser.annotations

# //= compliance/duvet-specification.txt#2.3.1
# //= type=implication
# //# This identifier of meta parts MUST
# //# be configurable.

# //= compliance/duvet-specification.txt#2.3.1
# //= type=implication
# //# If a second meta line exists it MUST start with "type=".
