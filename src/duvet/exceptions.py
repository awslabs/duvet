# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Exceptions Duvet could raise."""


class _BaseDuvetException(Exception):
    """The base-base of all Duvet exceptions."""


class ConfigError(_BaseDuvetException):
    """A problem with a config file, or a value in one."""
