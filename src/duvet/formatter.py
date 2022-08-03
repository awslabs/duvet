# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Formatter used by duvet-python."""
# Common sentence dividers
SENTENCE_DIVIDER = [". ", "! ", ".\n", "!\n", "? ", "?\n"]


def clean_content(content: str) -> str:
    """Create clean content string."""

    cleaned_content = " ".join(content.split())
    return cleaned_content


def split_long(para: str) -> list[str]:
    """Split long sentences."""

    lines = []
    line = ""
    for sentence in (s.strip() + "." for s in para.split(".")[:-1]):
        if len(line) + len(sentence) + 1 >= 80:  # can't fit on that line => start new one
            lines.append(line)
            line = sentence
        else:  # can fit on => add a space then this sentence
            line += " " + sentence
    return lines
