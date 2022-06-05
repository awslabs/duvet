# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Functional test suite for duvet.markdown."""
from pathlib import Path

import pytest

from duvet.markdown import MarkdownSpecification

pytestmark = [pytest.mark.functional, pytest.mark.local]


class TestMarkdownSpecification:
    @staticmethod
    @pytest.mark.xfail
    def test_dogfood(pytestconfig):
        # Currently, fails due to MarkdownSpecification assumption 4.
        # Once 4 is addressed, may fail for others as well.
        filepath: Path = pytestconfig.rootpath.joinpath("duvet-specification", "duvet-specification.md")
        duvet_spec: MarkdownSpecification = MarkdownSpecification(filepath)
        # TODO: Once Markdown assumptions 2-4 are addressed, add assertions for duvet_spec
