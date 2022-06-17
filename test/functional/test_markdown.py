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
        # TODO: add assertions for duvet_spec
