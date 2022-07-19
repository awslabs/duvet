# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Integration test suite for duvet.rfc."""
from pathlib import Path

import pytest

from duvet.rfc import RFCSpecification

from .integration_test_utils import get_path_to_esdk_dafny, ESDK_SPEC_RFC_PATTERNS, ESDK_SPEC_FILE_COUNT  # isort:skip

pytestmark = [pytest.mark.integ]


class TestRFCSpecificationAgainstESDK:
    def test(self):
        esdk_path: Path = get_path_to_esdk_dafny()
        esdk_specs = [
            RFCSpecification.parse(file) for pattern in ESDK_SPEC_RFC_PATTERNS for file in esdk_path.glob(pattern)
        ]
        assert len(esdk_specs) == ESDK_SPEC_FILE_COUNT
        assert all(hdr.validate() for spec in esdk_specs for hdr in spec.descendants)
