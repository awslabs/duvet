# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Utility functions to handle configuration for integration tests."""
import os
import pathlib
from typing import Optional

PATH_TO_ESDK_SPEC_KEY = "PATH_TO_ESDK_SPEC"
ESDK_SPEC_MD_PATTERNS = ["framework/**/*.md", "client-apis/*.md", "data-format/*.md"]


def _get_env_key(key: str) -> str:
    value: Optional[str] = os.environ.get(key)
    if value is None:
        raise ValueError(f"Environment variable {key} must be set to run this test.")
    return value


def get_path_to_esdk_spec() -> pathlib.Path:
    """Retrieves path to AWS Encryption SDK Specification"""
    value: str = _get_env_key(PATH_TO_ESDK_SPEC_KEY)
    return pathlib.Path(value)
