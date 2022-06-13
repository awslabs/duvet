# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Unit tests for ``duvet._config``."""
from typing import Dict

import pytest
from duvet._config import Config

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
[[spec.markdown]]
patterns = ["project-specification/**/*.md"]
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

    assert test.implementation == {
        "dfy": {"patterns": ["src/**/*.dfy", "test/**/*.rs", "compliance_exceptions/**/*.txt"]},
        "rs": {
            "comment-style": {"content": "//#", "meta": "//="},
            "patterns": ["src/**/*.rs", "test/**/*.rs", "compliance_exceptions/**/*.txt"],
        },
    }

    assert test.spec == {"markdown": [{"patterns": ["project-specification/**/*.md"]}]}
