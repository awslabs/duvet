# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Utilities for testing Duvet."""
import pathlib

def populate_file(tmp_path: pathlib.Path, content: str, filename: str) -> pathlib.Path:
    filepath = tmp_path.joinpath(filename)
    filepath.parent.mkdir(exist_ok=True, parents=True)
    with open(filepath, mode="w") as open_file:
        open_file.write(content)
    return filepath
