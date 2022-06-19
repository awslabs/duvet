# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Functional test suite for duvet.markdown."""
from pathlib import Path

import pytest

from duvet.markdown import MarkdownSpecification

from .integration_test_utils import get_path_to_esdk_spec, ESDK_SPEC_MD_PATTERNS  # isort:skip

pytestmark = [pytest.mark.integ]


class TestMarkdownSpecificationAgainstESDK:

    @staticmethod
    def test():
        esdk_path: Path = get_path_to_esdk_spec()
        esdk_specs = [
            MarkdownSpecification.parse(file)
            for pattern in ESDK_SPEC_MD_PATTERNS
            for file in esdk_path.glob(pattern)
        ]
        assert len(esdk_specs) == 33  # there are 33 markdown specifications in the ESDK spec
        assert all(hdr.validate() for spec in esdk_specs for hdr in spec.descendants)
