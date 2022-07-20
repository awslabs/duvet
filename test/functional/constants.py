# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Constants for functional tests"""

DUVET_SPEC_SECTION_COUNT = 27
DUVET_SPEC_FIRST_HEADER_TITLE = "Duvet specification"
DUVET_SPEC_FIRST_HEADER_BODY = "\n\n"

REQUIREMENT_BLOCK = """# Duvet specification

## Introduction

Duvet is an application to build confidence that your software is correct.

## Specification

A specification is a document, like this, that defines correct behavior.
This behavior is defined in regular human language.

### Section

The top level header for requirements is the name of a section.
The name of the sections MUST NOT be nested.
A requirements section MUST be the top level containing header.
A header MUST NOT itself be a requirement.
"""
