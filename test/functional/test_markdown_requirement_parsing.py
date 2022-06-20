# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Functional testing for requirement parsing"""

import pytest

# from duvet._config import Config, ImplConfig
#
# from ..utils import populate_file  # isort:skip

pytestmark = [pytest.mark.local, pytest.mark.functional]

# def test_extract_python_no_implementation_annotation(pytestconfig):
#     path = pytestconfig.rootpath.joinpath("src/duvet/identifiers.py")
#     filepath: Path = pytestconfig.rootpath.joinpath("duvet-specification", "duvet-specification.md")
#     duvet_spec: MarkdownSpecification = MarkdownSpecification(filepath)
# anno_meta_style = "# //="
# anno_content_style = "# //#"
# # Verify warning
# with pytest.warns(UserWarning) as record:
#     AnnotationParser([path], anno_meta_style, anno_content_style).extract_implementation_file_annotations()
# # check that only one warning was raised
# assert len(record) == 1
# # check that the message matches
# assert record[0].message.args[0] == (f"{path} do not have any annotations. Skipping file")
