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
            in_completes = [
                entry for entry in total if entry.status.name in ["COMPLETE", "EXCEPTION"]
            ]  # we don't care completes
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
