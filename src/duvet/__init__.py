# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""duvet-python."""

from .cli import cli

__all__ = ("__version__", "cli")

__version__ = "1.0.0"
_DEBUG = "INPUT_DEBUG"
_CONFIG_FILE = "INPUT_CONFIG-FILE"
