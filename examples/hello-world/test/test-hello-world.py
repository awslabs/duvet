# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Test Hello World."""
import pytest


# //= compliance/hello-world.txt#2.1
# //= type=test
# //# Python project MUST print hello.
def testhello_world(capfd):
    """Intentionally using different name to demo the exceptions."""
    # //= compliance/hello-world.txt#2.1
    # //= type=test
    # //# Python Project MUST print world.
    out, err = capfd.readouterr()
    assert out == "Hello World!"
