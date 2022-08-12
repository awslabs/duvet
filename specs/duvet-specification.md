[//]: # "Copyright Amazon.com Inc. or its affiliates. All Rights Reserved."
[//]: # "SPDX-License-Identifier: CC-BY-SA-4.0"

# Duvet specification

## Introduction

Duvet is an application to build confidence that your software is correct.
The first step in correct software is to document what correct behavior is.
This document is called a specification.
A specification can be a design document or an RFC.
This specification document describes an application’s behaviors.
What steps are important, in what order and why.
Duvet lets you annotate your code with text from your specification.
This helps clarify what a specific implementation should be doing and why.
Any part of the specification can be an annotation.
However, Duvet treats RFC 2119 keywords in your specification as requirement key-words that must be annotated.

Duvet reads files you designate as specifications and files you designate as part of your software.
It matches the annotations in your software to your specification. Duvet will then report on these matches.
Are there annotations in your source that do not exist in your specification?
Does every cited requirement from your specification have a test?
This report can either be a pass/fail for CI or an interactive report for development and code review.

## Specification

A specification is a document, like this, that defines correct behavior.
This behavior is defined in regular human language.

### Section

The top level header for requirements is the name of a section.
The name of the sections MUST NOT be nested.
A requirements section MUST be the top level containing header.
A header MUST NOT itself be a requirement.

A section MUST be indexable by combining different levels of naming.
This means that Duvet needs to be able to locate it uniquely within a specification.
A good example of a section is a header in an HTML or Markdown document.

### Requirement

Any complete sentence containing at least one RFC 2119 keyword MUST be treated as a requirement.
A requirement MAY contain multiple RFC 2119 keywords.
A requirement MUST be terminated by one of the following:

- period (.)
- exclamation point (!)
- list
- table

In the case of requirement terminated by a list,
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

In the case of requirement terminated by a table,
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

- markdown
- ietf

#### Toml

Duvet SHOULD be able to parse requirements formatted as Toml files.

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

The Meta: Location’s for Annotations targeting specifications
written in Markdown will NOT be identical to Locations targeting specifications written in IETF.

### Meta: Type

If the meta part is a single line then the type MUST be citation.
If a second meta line exists it MUST start with `type=`.
The type MUST be a valid annotation type string:

- citation
- test
- untestable
- exception
- implication
- todo

### Meta: Reason

A third meta line MAY exist: Reason. It MUST start with `reason=`.
The rest of this line and the following meta lines MUST be parsed
as the annotation’s reason until there are no more meta lines.

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
- Todo: The suggested location for the implementation.

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
