# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Requirement Parser used by duvet-python."""
import logging
import re
import warnings
from typing import List, Optional

import attr
from attrs import define, field

# Common sentence dividers
SENTENCE_DIVIDER = [". ", "! ", ".\n", "!\n", "? ", "?\n"]
# Common sentence dividers would mix up words
ALPHABETS = r"([A-Za-z])"
PREFIXES = r"(Mr|St|Mrs|Ms|Dr)[.]"
SUFFIXES = r"(Inc|Ltd|Jr|Sr|Co)"
STARTERS = r"(Mr|Mrs|Ms|Dr|He\s|She\s|It\s|They\s|Their\s|Our\s|We\s|But\s|However\s|That\s|This\s|Wherever)"
ACRONYMS = r"([A-Z][.][A-Z][.](?:[A-Z][.])?)"
WEBSITES = r"[.](com|net|org|io|gov)"
STOP_SIGN = "<stop>"


def preprocess_text(inline_text: str) -> str:
    """Take a chunk of inline requirement string and return a labeled string."""
    processed_text = "<stop> " + inline_text + "  <stop>"
    processed_text = processed_text.replace("\n", " ")
    processed_text = re.sub(PREFIXES, "\\1<prd>", processed_text)
    processed_text = re.sub(WEBSITES, "<prd>\\1", processed_text)
    if "Ph.D" in processed_text:
        processed_text = processed_text.replace("Ph.D.", "Ph<prd>D<prd>")
    processed_text = re.sub(r"\s" + ALPHABETS + "[.] ", " \\1<prd> ", processed_text)
    processed_text = re.sub(ACRONYMS + " " + STARTERS, "\\1<stop> \\2", processed_text)
    processed_text = re.sub(
        ALPHABETS + "[.]" + ALPHABETS + "[.]" + ALPHABETS + "[.]", "\\1<prd>\\2<prd>\\3<prd>", processed_text
    )
    processed_text = re.sub(ALPHABETS + "[.]" + ALPHABETS + "[.]", "\\1<prd>\\2<prd>", processed_text)
    processed_text = re.sub(" " + SUFFIXES + "[.] " + STARTERS, " \\1<stop> \\2", processed_text)
    processed_text = re.sub(" " + SUFFIXES + "[.]", " \\1<prd>", processed_text)
    processed_text = re.sub(" " + ALPHABETS + "[.]", " \\1<prd>", processed_text)
    if "”" in processed_text:
        processed_text = processed_text.replace(".”", "”.")
    if '"' in processed_text:
        processed_text = processed_text.replace('."', '".')
    if "!" in processed_text:
        processed_text = processed_text.replace('!"', '"!')
    if "?" in processed_text:
        processed_text = processed_text.replace('?"', '"?')
    processed_text = (
        processed_text.replace(". ", ". <stop>")
        .replace("? ", "? <stop>")
        .replace("! ", "! <stop>")
        .replace(".\n", ".\n<stop>")
        .replace("?\n", "?\n<stop>")  # noqa: E131
        .replace("!\n", "!\n<stop>")
        .replace("<prd>", ".")
    )
    return processed_text


def clean_content(content: str) -> str:
    """Create clean content string."""

    cleaned_content = content.replace("\n", " ").strip()
    return cleaned_content
