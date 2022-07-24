# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Formatter used by duvet-python."""
import re

# Common sentence dividers
SENTENCE_DIVIDER = [". ", "! ", ".\n", "!\n", "? ", "?\n"]
# Common sentence dividers would mix up words
ALPHABETS = r"([A-Za-z])"
PREFIXES = r"(Mr|St|Mrs|Ms|Dr)[.]"
SUFFIXES = r"(Inc|Ltd|Jr|Sr|Co)"
STARTERS = r"(Mr|Mrs|Ms|Dr|He\s|She\s|It\s|They\s|Their\s|Our\s|We\s|But\s|However\s|That\s|This\s|Wherever)"
ACRONYMS = r"([A-Z][.][A-Z][.](?:[A-Z][.])?)"
WEBSITES = r"[.](com|net|org|io|gov)"
STOP_SIGN = "<STOP>"


def preprocess_text(inline_text: str) -> str:
    """Take a chunk of inline requirement string and return a labeled string."""
    processed_text = " ".join([STOP_SIGN, inline_text, STOP_SIGN])
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
    processed_text = processed_text.replace(". ", ". " + STOP_SIGN)
    processed_text = processed_text.replace("? ", "? " + STOP_SIGN)
    processed_text = processed_text.replace("! ", "! " + STOP_SIGN)
    processed_text = processed_text.replace(".\n", ".\n" + STOP_SIGN)
    processed_text = processed_text.replace("?\n", "?\n" + STOP_SIGN)
    processed_text = processed_text.replace("!\n", "!\n" + STOP_SIGN)
    processed_text = processed_text.replace("<prd>", ".")

    return processed_text


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