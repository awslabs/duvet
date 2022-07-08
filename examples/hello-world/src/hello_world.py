# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Hello World used by duvet-python."""


# //= compliance/hello-world.txt#2.1
# //= type=implication
# //# Python project MUST print hello.
def hello_duvet():
    """Intentionally using different name to demo the exceptions."""
    # //= compliance/hello-world.txt#2.1
    # //= type=exception
    # //# Python project MUST print world.
    print("hello duvet.")
