# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Utility functions to handle configuration for integration tests."""
import os
from pathlib import Path
from typing import Optional

PATH_TO_ESDK_DAFNY_KEY = "PATH_TO_ESDK_DAFNY"
ESDK_SPEC_MD_PATTERNS = ["framework/**/*.md", "client-apis/*.md", "data-format/*.md"]
ESDK_SPEC_RFC_PATTERNS = ["compliance/**/*.txt"]
ESDK_SPEC_FILE_COUNT = 33  # there are 33 markdown specifications in the ESDK spec


def _get_env_key(key: str) -> str:
    value: Optional[str] = os.environ.get(key)
    if value is None:
        raise ValueError(f"Environment variable {key} must be set to run this test.")
    return value


def get_path_to_esdk_dafny() -> Path:
    """Retrieves path to AWS Encryption SDK Dafny"""
    value: str = _get_env_key(PATH_TO_ESDK_DAFNY_KEY)
    return Path(value)
