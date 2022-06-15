# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Unit tests for ``duvet._config``."""


import pytest

from duvet._config import ImplConfig

pytestmark = [pytest.mark.local, pytest.mark.unit]




def test_impl_config():
    try:
        ImplConfig([], "//=", "//=")
    except TypeError as error:
        # Verify the config function by checking the error message.
        assert repr(error) == ("TypeError('Meta style and Content style of annotation cannot be same.')")

    try:
        ImplConfig([], "/", "//=")
    except TypeError as error:
        # Verify the config function by checking the error message.
        assert repr(error) == ("TypeError('AnnotationPrefixes must have 3 or more characters')")

    try:
        ImplConfig([], "   ", "//=")
    except TypeError as error:
        # Verify the config function by checking the error message.
        assert repr(error) == ("TypeError('AnnotationPrefixes must not be all whitespace')")
    try:
        ImplConfig([], 123, "//=")
    except TypeError as error:
        # Verify the config function by checking the error message.
        assert repr(error) == ("TypeError('AnnotationPrefixes must be string')")
