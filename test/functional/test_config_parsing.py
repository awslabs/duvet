# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Functional testing for config parsing"""

import pytest

from duvet._config import Config, ImplConfig
from duvet.exceptions import ConfigError

from ..utils import populate_file  # isort: skip

pytestmark = [pytest.mark.local, pytest.mark.functional]

SPEC_BLOCK = """[spec.markdown]
patterns = ["project-specification/**/*.md"]"""

IMPL_BLOCK = """[implementation]
[implementation.rs]
patterns = ["src/**/*.rs", "test/**/*.rs", "compliance_exceptions/**/*.txt"]
comment-style = { meta = "//=", content = "//#" }
[implementation.dfy]
patterns = ["src/**/*.dfy", "test/**/*.dfy", "compliance_exceptions/**/*.txt"]"""

REPORT_BLOCK = """[report]
[report.blob]
url = ["https://github.com/aws/aws-encryption-sdk-dafny/blob/"]
[report.issue]
url = ["https://github.com/aws/aws-encryption-sdk-dafny/issues"]"""


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
[report.blob]
url = "https://github.com/aws/aws-encryption-sdk-dafny/blob/"
[report.issue]
url = "https://github.com/awslabs/duvet/issues"
[mode]
legacy = true
        """
    )


@pytest.mark.parametrize("contents", _config_test_cases())
def test_config_parse(tmpdir, contents: str):
    source = tmpdir.join("source")
    source.write(contents)
    with pytest.warns(UserWarning) as record:
        actual = Config.parse(str(source))
    assert len(record) == 7
    expected_impl_config = ImplConfig(impl_filenames=[])
    assert actual.implementation_configs == [expected_impl_config, expected_impl_config]
    assert not actual.specs


def test_missing_keys(tmp_path):
    try:
        Config.parse(populate_file(tmp_path, IMPL_BLOCK, "duvet.toml"))
    except ConfigError as error:
        # Verify the config function by checking the error message.
        assert repr(error) == ("ConfigError('Specification Config not found.')")

    try:
        Config.parse(populate_file(tmp_path, SPEC_BLOCK, "duvet.toml"))
    except ConfigError as error:
        # Verify the config function by checking the error message.
        assert repr(error) == ("ConfigError('Implementation Config not found.')")

    try:
        Config.parse(populate_file(tmp_path, "\n".join([SPEC_BLOCK, IMPL_BLOCK]), "duvet.toml"))
    except ConfigError as error:
        # Verify the config function by checking the error message.
        assert repr(error) == ("ConfigError('Report Config not found.')")


def test_valid_files(tmp_path):
    populate_file(tmp_path, "# spec1", "project-specification/spec1.md")
    populate_file(tmp_path, "# spec3", "project-specification/spec2/spec3.md")
    populate_file(tmp_path, "# spec4", "project-specification/spec4.md")
    # Verify that missing implementation will not interrupt file running
    populate_file(tmp_path, "# spec1", "src/spec1.dfy")
    populate_file(tmp_path, "# spec2", "src/spec2.rs")
    populate_file(tmp_path, "# spec3", "test/test_spec1.dfy")

    actual_path = populate_file(tmp_path, "\n".join([SPEC_BLOCK, IMPL_BLOCK, REPORT_BLOCK]), "duvet_config.toml")
    with pytest.warns(UserWarning) as record:
        actual_config = Config.parse(actual_path)
    assert len(record) == 3
    # Verify the correctness of the Config object by checking the length.
    assert len(actual_config.implementation_configs) == 2
    assert len(actual_config.specs) == 3
