# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Functional test suite for duvet.markdown."""
from pathlib import Path

import pytest

from duvet.markdown import MarkdownSpecification

pytestmark = [pytest.mark.functional, pytest.mark.local]


class TestMarkdownSpecification:
    @staticmethod
    def test_dogfood(pytestconfig):
        filepath: Path = pytestconfig.rootpath.joinpath("duvet-specification", "duvet-specification.md")
        duvet_spec: MarkdownSpecification = MarkdownSpecification(filepath)
        assert duvet_spec.filepath == filepath
        assert len(duvet_spec.headers) == 1
        assert duvet_spec.cursor.title == "Duvet specification"
        assert len(duvet_spec.cursor.descendants) == 27
        assert all(hdr.validate() for hdr in duvet_spec.cursor.descendants)
        assert duvet_spec.cursor.get_body() == "\n\n"
