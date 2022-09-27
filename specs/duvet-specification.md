[//]: # "Copyright Amazon.com Inc. or its affiliates. All Rights Reserved."
[//]: # "SPDX-License-Identifier: CC-BY-SA-4.0"

# Duvet specification

## Version

0.2.0

### Changelog

- 0.2.0

  - Initial record

- 0.1.0

  - "Specless" Rust Implementation

## Overview

This document introduces and describes Duvet.

Any implementation of Duvet MUST follow this specification.

## Introduction

Duvet is an application to build confidence that your software is correct.
The first step in correct software is to document what correct behavior is.
This document is called a specification.
A specification can be a design document or an RFC.
This specification document describes an application’s behaviors.
It includes which steps are important, in what order, and why.
Duvet lets you annotate your code with text from your specification.
This helps clarify what a specific implementation should be doing and why.
Any part of the specification can be an annotation.
However, Duvet treats RFC 2119 keywords in your specification as requirement key-words that must be annotated.

Duvet reads files you designate as specifications and files you designate as part of your software.
It matches the annotations in your software to your specification. Duvet will then report on these matches.
Are there annotations in your source that do not exist in your specification?
Does every cited requirement from your specification have a test?
This report can either be a pass/fail for CI or an interactive report for development and code review.

### Conventions used in this document

The keywords
"MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL"
in this document are to be interpreted as described in [RFC2119](https://tools.ietf.org/html/rfc2119).

# Structures

This following sections describe the common Duvet structures and their behavior.

## Specification

A specification is a document, like this, that defines correct behavior.
This behavior is defined in regular human language.

## Section

The top-level header for requirements is the name of a section.
After the section's header, follows the body.
Requirements defined inside the body MUST be associated to the immediate section in which they are defined.
This means requirements have one and only one section that they are associated with.
A header MUST NOT itself be a requirement.

A section MUST be indexable by combining different levels of naming.
This means that Duvet needs to be able to locate it uniquely within a specification.
A good example of a section is a header in an HTML or Markdown document.

## Requirement

Any complete sentence containing at least one RFC 2119 keyword MUST be treated as a requirement.
A requirement MAY contain multiple RFC 2119 keywords.
A requirement MUST be terminated by one of the following:

- period (.)
- exclamation point (!)
- list
- table

In the case of the requirement being terminated by a list,
the text proceeding the list MUST be concatenated
with each element of the list to form a requirement.
Taking the above list as an example,
Duvet is required to be able to recognize 4 different ways
to group text into requirements.
List elements MAY have RFC 2119 keywords,
this is the same as regular sentences with multiple keywords.
Sublists MUST be treated as if the parent item were terminated by the sublist.
List elements MAY contain a period (.) or exclamation point (!)
and this punctuation MUST NOT terminate the requirement by
excluding the following elements from the list of requirements.

In the case of the requirement being terminated by a table,
the text proceeding the table SHOULD be concatenated
with each row of the table to form a requirement.
Table cells MAY have RFC 2119 keywords,
this is the same as regular sentences with multiple keywords.
Table cells MAY contain a period (.) or exclamation point (!)
and this punctuation MUST NOT terminate the requirement
by excluding the following rows from the table of requirements.

### Legacy Requirement

Older versions of Duvet were more restrictive in parsing requirements.
They did not treat elements of lists or rows in a table as individual elements.
For backwards compatibility Duvet MUST support
this older simpler form of requirement identification.
Any complete sentence containing at least one RFC 2119 keyword MUST be treated as a requirement.
A requirement MAY contain multiple RFC 2119 keywords.
A requirement MUST be terminated by one of the following:

- period (.)
- exclamation point (!)
- an empty blank line

The main distinction between this legacy and regular requirement identification
is that there is no sugar for lists or tables.
For a given a specification Duvet MUST use one way to identify requirements.

### Formats

Duvet MUST be able to parse specifications formatted as:

- Markdown
- IETF style RFCs as text files.

#### Requirements to TOML

Duvet SHOULD be able to record parsed requirements into Toml Files.

These Toml features supports users of Duvet who do not author the specifications they are implementing.
As such, they need to customize the extracted requirements,
modifying the content of requirements or adding/removing requirements.

## Annotation

Annotations are references to a text from a section in a specification,
written as comment in the source code and test code.
Annotations are generally stored as formatted comments in source within a project.

### Meta

The default identifier for the meta part in source documents MUST be //= followed by a single space.
This identifier of meta parts MUST be configurable.

### Meta: Location

The first line of the meta part identifies the location of the content,
it MUST be parsed as a URL.
All parts of the URL other than a URL fragment MUST be optional
and MUST identify the specification that contains this section and content.
The URL MUST contain a URL fragment that uniquely identifies
the section that contains this content.
If the URL only contains a URL fragment
then this content only exists as an annotation.
Such comments are useful to use Duvet to manage implementation specific requirements
that do not exist in a specification.

The Meta: Locations for Annotations targeting specifications
written in Markdown will NOT be identical to Locations targeting specifications written in IETF.

### Meta: Key/Values parsing

After the [Meta: Location](#meta-location) all additional meta data is a series of name/value pairs.
The name MUST be the characters between the meta identifier and the first `=`.
The value MUST be all characters after the first `=`.
If consecutive duplicate names exist in a meta section
the values MUST be concatenated with a `\n`.
### Meta: Type

If the meta part is a single line then the type MUST be citation.
The type MUST be a valid annotation type string:

- citation
- test
- untestable
- exception
- implication
- todo

### Meta: Reason

The reason tag MUST start with `reason=`.

### Annotation Types

Annotation types give meaning to the way the thing being annotated relate to the content.
Each type is listed here with its intended usage.

- Citation: The implementation of what is described in the content.
- Test: A test or test vector that verifies that an implementation honors what is described in the content.
  These tests are ideally negative.
  i.e. Counter examples to the description are attempted and fail.
- Untestable: The implementation that can not be tested.
  Some runtimes, languages, or constructions make the idea described in the content untestable.
  Additional protections against random bit flips is a good example.
- Deviation: An implementation that differs from what is described in the content.
  The implementation may have proceeded the specification for example.
- Exception: A part of a specification that is not implemented.
  This can include optional or legacy features.
  Exceptions have an optional reason field.
- Implication: A construction that is correct by construction i.e. it can not fail.
  For example take a requirement that a function take a specific set of arguments.
  In a static strongly typed language the arguments of a function can not change.
  So an implication could be a good choice to express that the implementation satisfies this requirement.
- TODO: The suggested location for the implementation.

### Content

A one or more line meta part MUST be followed by at least a one line content part.
The default identifier for the content part in software documents
MUST be `//#` followed by a single space.
This identifier of content parts MUST be configurable.
All content part lines MUST be consecutive.

## Matching

Duvet needs to be able to match annotation content.
Both to other annotations and to specifications.
This matching is used to report on requirements.

### Matching annotation and specification requirement

For an annotation to match a specification
the annotation's content MUST exist in the specification's section
identified by the annotation's meta location URL.
The match between the annotation content and the specification text
MUST be case-sensitive but MUST NOT be white space sensitive
and MUST uniquely identify text in the specification.
For simple text in a paragraph this means just identifying
the annotation's content is a substring of the text in the specification's section.
Elements of a list MUST NOT be matched by their order within the list.
This means that an annotation may contain a list
that is a subset of the elements in the specification.
Rows of a table MUST NOT be matched by their order within the table.
This means that an annotation MAY contain a table that is a subset of the rows in the specification.

## Matching Labels

Duvet MUST analyze the matching annotations, generating Matching Labels.
Matching Labels MAY be exclusive.
Duvet MUST label requirements matched to annotations as follows:

### Implemented

A specification requirement MUST be labeled `Implemented`
if there exists at least one matching annotation of type:

- citation
- untestable
- implication

### Attested

A specification requirement MUST be labeled `Attested`
if there exists at least one matching annotation of type

- test
- untestable
- implication

### Excused

A specification requirement MUST be labeled `Excused`
and MUST only be labeled `Excused` if there exists
a matching annotation of type `exception` and the annotation has a `reason`.

### Unexcused

A specification requirement MUST be labeled `Unexcused`
and MUST only be labeled `Unexcused` if there exists
a matching annotation of type `exception`
and the annotation does NOT have a `reason`.

## Report

Duvet's report shows how your project conforms to specifications.
This lets you bound the correctness of your project.
As you annotate the code in your project
Duvet's report creates links between the implementation,
the specification,
and attestations.

Duvet’s report aids customers in annotating their code.

### Status

Duvet MUST analyze the matching labels for every requirement;
the result of this analysis is the requirement’s Status.
Requirement Statuses MUST be exclusive.

The Requirement Statuses MUST be:

- Complete - The requirement MUST have both the labels `Implemented` and `Attested`
- Missing Proof - The requirement MUST only have the label `Implemented`
- Excused - The requirement MUST only have the label `Excused`
- Missing Implementation - The requirement MUST only have the label `Attested`
- Not started - The requirement MUST NOT have any labels
- Missing Reason - The requirement MUST have the label `Unexcused`
- Duvet Error - The requirements matching labels MUST be invalid.

[//]: # "TODO: Should `Duvet Error` trigger a warning/exception?"
[//]: # "TODO: Should `Duvet Error` cause a Fail?"

### Pass/Fail

For Duvet to pass the Status of every “MUST” and “MUST NOT” requirement MUST be Complete or Excused.<br>
Duvet MUST return `0` for Pass. Duvet SHOULD print a success message.<br>
Duvet MUST NOT return `0` for Fail. Duvet SHOULD print a failure message.<br>

### Report Summary

The report summary shows high level information about the linkage between annotations and specifications.
It MUST have all a link for each included specifications.
It MUST have all a link for annotations that do not match any included specifications.
It MUST have all a link for annotations not associated with any specifications.
For each link it MUST have a table summarizing
the matrix of requirements crossed annotation types,
and include totals for both sides.

### Specification Overview

The specification overview shows more detailed information about the specific specification.
It MUST have a table summarizing the matrix of requirements across annotation types,
and include totals for both sides.
It MUST show a table with a row for each requirement.
The table MUST have a column for:

- Section within the specification - it would be heading in the markdown, section name and number for ietf docs
- Requirement key word - key word defined in rfc2119
- Status
- Text - The requirement text

### Specification Section

The specification section shows the specific specification text and how this links to annotation.
It MUST show all text from the section.
It MUST highlight the text for every requirement.
It MUST highlight the text that matches any annotation.
Any highlighted text MUST have a mouse over that shows its annotation information.
Clicking on any highlighted text MUST bring up a popup that shows:

- The requirement level
- The text
- List of quick links to add the text to a Duvet comment for every annotation type
- If annotations exist, relative links to these files. This link SHOULD include the line number.

Selecting any text of the specification
and right-clicking on it
MUST bring up a popup for the selected text that shows:

- The text
- List of quick links to add the text to a Duvet comment for every annotation type
- If annotations exist, relative links to these files. This link SHOULD include the line number.

It MUST show a table with a row for each requirement included in this section.
The table MUST have a column for:

- Section within the specification
- Requirement key word - key word defined in rfc2119
- Status
- Text - The requirement text

# Behaviors

Duvet MUST support a [CI Behavior](#ci-behavior).

Duvet SHOULD support a [Requirement to TOML Behavior](#record-requirements-as-toml-behavior).

[//]: # "TODO: Define all of behaviors input and output in one place"
[//]: # "TODO: Describe how Duvet should handle exceptions"

## CI Behavior

The following sections describe
how Duvet parses specifications and implementations
to generate a report and a pass/fail status appropriate
for continuous integration (CI) usage.

Implementations of Duvet MUST implement this behavior.

This MUST be the default execution of Duvet.

This behavior MUST accept a configuration file.

### Parse Specifications

Duvet MUST accept one or more groups of file patterns that describe the paths to the specifications files.

These file pattern groups MUST specify which [specification format](#formats) they are in.

For each file pattern group,
for each file pattern in the group,
Duvet MUST attempt to parse as a [specification](#specification) any files
discovered on this file pattern
as if they were in the file pattern groups' [specification format](#formats).

Failure to parse a file MUST NOT halt Duvet.

Failure to parse a file SHOULD yield a warning.

#### Specifications as TOML

In addition to parsing Markdown and RFC (`.txt`) files as specifications,
Duvet SHOULD accept one or more file patterns that describe the paths to `.toml` files.

Duvet SHOULD interpret each directory containing one or more TOML files as a [specification](#specification).

See [Sections as TOML](#sections-as-toml).

### Extract Sections

Duvet MUST extract [sections](#section) from [specifications](#specification).

#### Sections as TOML

If Duvet has interpreted TOML directories as [specifications](#specification),
Duvet SHOULD interpret each TOML file in a directory
as a [section](#section) of that directories' specification.

See [Requirements from TOML](#requirements-from-toml).

### Extract Requirements.

Duvet MUST extract [Requirements](#requirement) from [Sections](#section).

#### Requirements from TOML

If Duvet has interpreted TOML files as a [section](#section),
for every [array of tables](https://toml.io/en/v1.0.0#array-of-tables) in the TOML file,
Duvet SHOULD extract a [requirement](#requirement).

### Parse Implementation

Duvet MUST accept one or more file pattern groups that describe the paths to the implementation files.

Each file pattern group MAY be associated with an annotation identifier tuple,
which MUST be used when parsing files from the file pattern group.

Otherwise, the default annotation identifiers MUST be used for that file pattern group.

For each file pattern group,
for each file pattern in the group,
for every file found via a file pattern,
Duvet MUST extract [annotations](#annotation) form that file
with the group's annotation identifiers.

Failure to parse a file MUST NOT halt Duvet.

Failure to parse a file SHOULD yield a warning.

Duvet MUST attempt to match these [annotations](#annotation) to [requirements](#requirement)
as described in [Matching](#matching).

Even if a match is not found,
Duvet MUST record every [annotation](#annotation).

### Generate Report

Duvet MUST analyze every [requirement](#requirement) extracted,
generating and validating matching labels as described in [Matching Labels](#matching-labels).

Then, Duvet MUST determine every [requirement's](#requirement) [Status](#status).

Duvet MUST generate an HTML report as described in [report](#report).

### Pass or Fail

Duvet MUST Pass or Fail as described in [Pass/Fail](#Pass/Fail).

## Record Requirements as TOML Behavior

Duvet SHOULD support requirement to Toml extraction as a separate utility that MAY be invoked outside normal execution.

See Requirements from TOML, Sections as TOML, and Specifications as TOML.

[//]: # "TODO: Flesh this out"
