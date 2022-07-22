# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Unit tests for ``duvet._config``."""

import pytest

from duvet._config import ImplConfig
from duvet.exceptions import ConfigError

pytestmark = [pytest.mark.local, pytest.mark.unit]


def test_impl_config():
    try:
        ImplConfig([], "//=", "//=")
    except ConfigError as error:
        # Verify the config function by checking the error message.
        assert repr(error) == ("ConfigError('Meta style and Content style of annotation cannot be same.')")

    try:
        ImplConfig([], "/", "//=")
    except ConfigError as error:
        # Verify the config function by checking the error message.
        assert repr(error) == ("ConfigError('AnnotationPrefixes must have 3 or more characters')")

    try:
        ImplConfig([], "   ", "//=")
    except ConfigError as error:
        # Verify the config function by checking the error message.
        assert repr(error) == ("ConfigError('AnnotationPrefixes must not be all whitespace')")
    try:
        ImplConfig([], 123, "//=")
    except ConfigError as error:
        # Verify the config function by checking the error message.
        assert repr(error) == ("ConfigError('AnnotationPrefixes must be string')")
