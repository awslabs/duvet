# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Summary reporting."""

from typing import Optional

from attrs import define, field
from tabulate import tabulate

from duvet._config import Config
from duvet.identifiers import RequirementLevel
from duvet.structures import Report, Section

HEADERS = ["Section", "Requirement", "Total", "Incomplete"]


@define
class SummaryReport:
    """A reporter for writing the summary report."""

    report: Report = field(init=True)
    config: Optional[Config] = None
    outfile: Optional[str] = None

    # Requirement	Total	Complete	Citations	Implications	Tests	Exceptions	TODOs
    # MUST	57	27	34	12	0	15	1
    # SHOULD	9	8	0	2	0	6	1
    # MAY	5	5	0	0	0	5	0
    # Totals	71	40	34	14	0	26	2

    def analyze_report(self) -> bool:
        """Return report pass or fail."""

        # //= compliance/duvet-specification.txt#2.6.2
        # //# For Duvet to pass the Status of every "MUST" and "MUST NOT" requirement MUST be Complete or Excused.

        self.report.analyze_annotations()
        return self.report.report_pass

    @staticmethod
    def analyze_stats(section: Section) -> list[list]:
        """Given a section, return a table of analysis of section.

        Incomplete should be the only thing we care about during CI run.
        """
        section_analysis: list[dict] = []
        section.analyze_annotations()
        for level in RequirementLevel:
            total = [entry for entry in section.requirements.values() if entry.requirement_level.name == level.name]
            in_completes = [entry for entry in total if entry.status.name != "COMPLETE"]  # we don't care
            level_dict = {
                "Section": section.uri,
                "Requirement": level.name,
                "Total": len(total),
                "Incomplete": len(in_completes),
            }
            section_analysis.append(level_dict)

        # Return table of analysis
        return [list(level_dict.values()) for level_dict in section_analysis]

    @staticmethod
    def report_section(table: list[list]) -> str:
        """Report Section stats."""
        return tabulate(table, HEADERS, tablefmt="simple")


# //= compliance/duvet-specification.txt#2.5
# //# Duvet MUST analyze the matching annotations, generating Matching Labels.

# //= compliance/duvet-specification.txt#2.5
# //# Duvet MUST label requirements matched to annotations as follows:

# //= compliance/duvet-specification.txt#2.5
# //# Matching Labels MAY be exclusive.

# //= compliance/duvet-specification.txt#2.5.4
# //# A specification requirement MUST be labeled "Unexcused" and MUST only be labeled "Unexcused" if there
# //# exists a matching annotation of type "exception" and the annotation does NOT have a "reason".
