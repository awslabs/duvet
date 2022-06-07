# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Placeholder module to remind you to write tests."""
import pytest

import duvet


@pytest.mark.xfail(strict=True)
@pytest.mark.functional
def test_write_tests():
    assert bool(duvet.__version__)  # hack to meet flake8 F401
    assert False
