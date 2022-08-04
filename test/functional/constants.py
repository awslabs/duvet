# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Constants for functional test."""

TEST_SPEC_TOML_TARGET = """target = "../duvet-python/spec/spec.txt#2.2.1"    """

TEST_SPEC_TOML_COMMENT = """
# 2.2.1.  Section
#
# The top level header for requirements is the name of a section.  The
# name of the sections MUST NOT be nested.  A requirements section MUST
# be the top level containing header.  A header MUST NOT itself be a
# requirement.
# A section MUST be indexable by combining different levels of naming.
# This means that Duvet needs to be able to locate it uniquely within a
# specification.  A good example of a section is a header in an HTML or
# Markdown document.
"""

TEST_SPEC_TOML_SPEC = """
[[spec]]
level = "MUST"
quote = '''
The
name of the sections MUST NOT be nested.
'''

[[spec]]
level = "MUST"
quote = '''
A requirements section MUST
be the top level containing header.
'''

[[spec]]
level = "MUST"
quote = '''
A header MUST NOT itself be a
requirement.
'''

[[spec]]
level = "MUST"
quote = '''
A section MUST be indexable by combining different levels of naming.
'''

[[spec]]
level = "MUST"
quote = '''
The keyring MUST attempt to serialize the decryption materials'
(structures.md#decryption-materials) encryption context
(structures.md#encryption-context-1) in the same format as the
serialization of the message header AAD key value pairs (../data-
format/message-header.md#key-value-pairs).
'''
"""
