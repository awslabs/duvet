# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Parse a config file."""
import toml
from attrs import define

__all__ = ["Config"]


@define
class Config:
    """Duvet configuration container and parser."""

    implementation: dict
    spec: dict

    @classmethod
    def parse(cls, config_file_path: str) -> "Config":
        """Parse a config file."""
        with open(config_file_path, "r", encoding="utf-8") as config_file:
            parsed = toml.load(config_file)
        # Parse implementation and specification preset
        return Config(parsed["implementation"], parsed["spec"])
