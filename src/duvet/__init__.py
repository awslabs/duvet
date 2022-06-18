# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""duvet-python."""
from duvet.identifiers import __version__
from duvet.markdown import MarkdownSpecification
from duvet.spec_toml_parser import TomlRequirementParser

__all__ = ("__version__", "TomlRequirementParser", "MarkdownSpecification")
