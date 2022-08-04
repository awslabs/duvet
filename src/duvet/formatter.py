# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Formatter used by duvet-python."""


def clean_content(content: str) -> str:
    """Create clean content string."""

    cleaned_content = " ".join(content.split())
    return cleaned_content
