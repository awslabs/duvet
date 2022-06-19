# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Functional test suite for duvet.markdown."""
from pathlib import Path

import pytest

from duvet.markdown import MarkdownSpecification

from .constants import (  # isort:skip
    DUVET_SPEC_SECTION_COUNT,
    DUVET_SPEC_FIRST_HEADER_BODY,
    DUVET_SPEC_FIRST_HEADER_TITLE,
)

pytestmark = [pytest.mark.functional, pytest.mark.local]


class TestMarkdownSpecification:
    @staticmethod
    def test_dogfood(pytestconfig):
        filepath: Path = pytestconfig.rootpath.joinpath("duvet-specification", "duvet-specification.md")
        duvet_spec: MarkdownSpecification = MarkdownSpecification.parse(filepath)
        assert duvet_spec.filepath == filepath
        assert duvet_spec.title == "duvet-specification.md"
        assert len(duvet_spec.children) == 1
        duvet_spec.cursor = duvet_spec.children[0]
        assert duvet_spec.cursor.title == DUVET_SPEC_FIRST_HEADER_TITLE
        assert len(duvet_spec.cursor.descendants) == DUVET_SPEC_SECTION_COUNT
        assert all(hdr.validate() for hdr in duvet_spec.cursor.descendants)
        assert duvet_spec.cursor.get_body() == DUVET_SPEC_FIRST_HEADER_BODY
