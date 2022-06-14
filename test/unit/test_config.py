# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Unit tests for ``duvet._config``."""

from typing import Dict

import pytest

from duvet._config import Config, ImplConfig

from ..utils import populate_file  # isort:skip

pytestmark = [pytest.mark.local, pytest.mark.functional]


def _config_test_cases():
    yield (
        """
[implementation]
[implementation.rs]
patterns = ["src/**/*.rs", "test/**/*.rs", "compliance_exceptions/**/*.txt"]
comment-style = { meta = "//=", content = "//#" }
[implementation.dfy]
patterns = ["src/**/*.dfy", "test/**/*.rs", "compliance_exceptions/**/*.txt"]
[spec.markdown]
patterns = ["project-specification/**/*.md"]
[report]
blob = "https://github.com/aws/aws-encryption-sdk-dafny/blob/"
issue = "https://github.com/aws/aws-encryption-sdk-dafny/issues"
[mode]
legacy = true
        """,
        {
            "include": [],
            "exclude": [],
            "output-test": [],
            "prefix": [],
        },
    )


@pytest.mark.parametrize("contents, mapping", _config_test_cases())
def test_config_parse(tmpdir, contents: str, mapping: Dict[str, Config]):
    source = tmpdir.join("source")
    source.write(contents)
    test = Config.parse(str(source))
    test_impl_config = ImplConfig(impl_filenames=[])
    assert test.implementation_configs == [test_impl_config]
    assert not test.specs


def test_impl_config():
    try:
        ImplConfig([], "//=", "//=")
    except TypeError as error:
        # Verify the config function by checking the error message.
        assert repr(error) == ("TypeError('Meta style and Content style of annotation cannot be same.')")

    try:
        ImplConfig([], "/", "//=")
    except TypeError as error:
        # Verify the config function by checking the error message.
        assert repr(error) == ("TypeError('AnnotationPrefixes must have 3 or more characters')")

    try:
        ImplConfig([], "   ", "//=")
    except TypeError as error:
        # Verify the config function by checking the error message.
        assert repr(error) == ("TypeError('AnnotationPrefixes must not be all whitespace')")
    try:
        ImplConfig([], 123, "//=")
    except TypeError as error:
        # Verify the config function by checking the error message.
        assert repr(error) == ("TypeError('AnnotationPrefixes must be string')")


def test_missing_keys(tmp_path):
    impl_block = """[implementation]
[implementation.rs]
patterns = ["src/**/*.rs", "test/**/*.rs", "compliance_exceptions/**/*.txt"]
comment-style = { meta = "//=", content = "//#" }
[implementation.dfy]
patterns = ["src/**/*.dfy", "test/**/*.rs", "compliance_exceptions/**/*.txt"]"""
    try:
        Config.parse(populate_file(tmp_path, impl_block, "duvet_config.toml"))
    except ValueError as error:
        # Verify the config function by checking the error message.
        assert repr(error) == ("ValueError('Specification Config not found.')")

    spec_block = """[spec.markdown]
patterns = ["project-specification/**/*.md"]"""

    try:
        Config.parse(populate_file(tmp_path, spec_block, "duvet_config.toml"))
    except ValueError as error:
        # Verify the config function by checking the error message.
        assert repr(error) == ("ValueError('Implementation Config not found.')")

    try:
        Config.parse(populate_file(tmp_path, "\n".join([spec_block, impl_block]), "duvet_config.toml"))
    except ValueError as error:
        # Verify the config function by checking the error message.
        assert repr(error) == ("ValueError('Report Config not found.')")
