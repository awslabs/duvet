# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Annotation Parser used by duvet-python."""
# import pathlib
# import re

import pytest

# from duvet._config import DEFAULT_CONTENT_STYLE, DEFAULT_META_STYLE
# from duvet.annotation_parser import AnnotationBlock

pytestmark = [pytest.mark.unit, pytest.mark.local]
#
# ANNO_TYPE_REGEX = re.compile(DEFAULT_META_STYLE + r"[\s]type=" + r"(.*?)\n")
# ANNO_REASON_REGEX = re.compile(DEFAULT_META_STYLE + r"[\s]reason=" + r"(.*?)\n")
# ANNO_META_REGEX = re.compile(DEFAULT_META_STYLE + r"[\s](.*?)\n")
# ANNO_CONTENT_REGEX = re.compile(DEFAULT_CONTENT_STYLE + r"\s(.*?)\n")
#
# TEST_DAFNY_STR = (
#     "//= compliance/client-apis/client.txt#2.4.2.1\n"
#     "//= type=implication\n"
#     "//# * encrypt (encrypt.md) MUST only support algorithm suites that have\n"
#     "//# a Key Commitment (../framework/algorithm-suites.md#algorithm-\n"
#     "//# suites-encryption-key-derivation-settings) value of False\n"
# )
#
# expected_content = (
#     "* encrypt (encrypt.md) MUST only support algorithm suites that have\n"
#     "a Key Commitment (../framework/algorithm-suites.md#algorithm-\n"
#     "suites-encryption-key-derivation-settings) value of False\n"
# )
# expected_content = expected_content.replace("\n", " ").strip()
#
#
# def test_extract_annotation():
#     """Test a valid annotation string in dafny format."""
#     actual_anno_block = AnnotationBlock(
#         TEST_DAFNY_STR.splitlines(keepends=True),
#         0,
#         5,
#         pathlib.Path("test.dfy"),
#         ANNO_TYPE_REGEX,
#         ANNO_REASON_REGEX,
#         ANNO_META_REGEX,
#         ANNO_CONTENT_REGEX,
#     )
#     assert actual_anno.type.name == "IMPLICATION"
#     assert actual_anno.target == "compliance/client-apis/client.txt#2.4.2.1"
#     assert actual_anno.content == expected_content
#     assert actual_anno.uri == "$".join(["compliance/client-apis/client.txt#2.4.2.1", expected_content])
#
#
# def test_extract_annotation_content_block():
#     """Test a valid annotation block in dafny format."""
#     anno_content = (
#         "//# * encrypt (encrypt.md) MUST only support algorithm suites that have\n"
#         "//# a Key Commitment (../framework/algorithm-suites.md#algorithm-\n"
#         "//# suites-encryption-key-derivation-settings) value of False\n"
#     )
#     actual_anno = AnnotationBlock(
#         anno_content.splitlines(keepends=True),
#         0,
#         5,
#         pathlib.Path("test.dfy"),
#         ANNO_TYPE_REGEX,
#         ANNO_REASON_REGEX,
#         ANNO_META_REGEX,
#         ANNO_CONTENT_REGEX,
#     ).to_annotation()
#     assert actual_anno is None
#
#
# def test_citation():
#     anno_meta_content = (
#         "//= compliance/client-apis/client.txt#2.4.2.1\n"
#         "//# * encrypt (encrypt.md) MUST only support algorithm suites that have\n"
#         "//# a Key Commitment (../framework/algorithm-suites.md#algorithm-\n"
#         "//# suites-encryption-key-derivation-settings) value of False\n"
#     )
#     actual_anno = AnnotationBlock(
#         anno_meta_content.splitlines(keepends=True),
#         0,
#         5,
#         pathlib.Path("test.dfy"),
#         ANNO_TYPE_REGEX,
#         ANNO_REASON_REGEX,
#         ANNO_META_REGEX,
#         ANNO_CONTENT_REGEX,
#     ).to_annotation()
#     assert actual_anno.type.name == "CITATION"
#     assert actual_anno.target == "compliance/client-apis/client.txt#2.4.2.1"
#     assert actual_anno.content == expected_content
#     assert actual_anno.uri == "$".join(["compliance/client-apis/client.txt#2.4.2.1", expected_content])
#
#
# def test_reasoned_exception():
#     reasoned_exception = (
#         "//= compliance/client-apis/client.txt#2.4.2.1\n"
#         "//= type=exception\n"
#         "//= reason=This is a reason.\n"
#         "//# * encrypt (encrypt.md) MUST only support algorithm suites that have\n"
#         "//# a Key Commitment (../framework/algorithm-suites.md#algorithm-\n"
#         "//# suites-encryption-key-derivation-settings) value of False\n"
#     )
#     actual_anno = AnnotationBlock(
#         reasoned_exception.splitlines(keepends=True),
#         0,
#         5,
#         pathlib.Path("test.dfy"),
#         ANNO_TYPE_REGEX,
#         ANNO_REASON_REGEX,
#         ANNO_META_REGEX,
#         ANNO_CONTENT_REGEX,
#     ).to_annotation()
#     assert actual_anno.type.name == "EXCEPTION"
#     assert actual_anno.reason == "This is a reason."
#     assert actual_anno.has_reason
#
#
# def test_long_reasoned_exception():
#     reasoned_exception = (
#         "//= compliance/client-apis/client.txt#2.4.2.1\n"
#         "//= type=exception\n"
#         "//= reason=This is a reason\n"
#         "//= a super super long reason.\n"
#         "//# * encrypt (encrypt.md) MUST only support algorithm suites that have\n"
#         "//# a Key Commitment (../framework/algorithm-suites.md#algorithm-\n"
#         "//# suites-encryption-key-derivation-settings) value of False\n"
#     )
#     actual_anno = AnnotationBlock(
#         reasoned_exception.splitlines(keepends=True),
#         0,
#         5,
#         pathlib.Path("test.dfy"),
#         ANNO_TYPE_REGEX,
#         ANNO_REASON_REGEX,
#         ANNO_META_REGEX,
#         ANNO_CONTENT_REGEX,
#     ).to_annotation()
#     assert actual_anno.type.name == "EXCEPTION"
#     assert actual_anno.reason == "This is a reason a super super long reason."
#     assert actual_anno.target == "compliance/client-apis/client.txt#2.4.2.1"
#     assert actual_anno.has_reason
#
#
# def test_unreasoned_exception():
#     not_reasoned_exception = (
#         "//= compliance/client-apis/client.txt#2.4.2.1\n"
#         "//= type=exception\n"
#         "//# * encrypt (encrypt.md) MUST only support algorithm suites that have\n"
#         "//# a Key Commitment (../framework/algorithm-suites.md#algorithm-\n"
#         "//# suites-encryption-key-derivation-settings) value of False\n"
#     )
#     actual_anno = AnnotationBlock(
#         not_reasoned_exception.splitlines(keepends=True),
#         0,
#         5,
#         pathlib.Path("test.dfy"),
#         ANNO_TYPE_REGEX,
#         ANNO_REASON_REGEX,
#         ANNO_META_REGEX,
#         ANNO_CONTENT_REGEX,
#     ).to_annotation()
#     assert actual_anno.type.name == "EXCEPTION"
#     assert not actual_anno.has_reason
