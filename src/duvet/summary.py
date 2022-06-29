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
class SummaryReporter:
    """A reporter for writing the summary report."""

    report: Report = field(init=True)
    config: Config = field(init=True)
    outfile: Optional[str] = None
    fmt_err = "%s   %s: %s"

    # Requirement	Total	Complete	Citations	Implications	Tests	Exceptions	TODOs
    # MUST	57	27	34	12	0	15	1
    # SHOULD	9	8	0	2	0	6	1
    # MAY	5	5	0	0	0	5	0
    # Totals	71	40	34	14	0	26	2

    # @define
    # class SectionSummary:
    #     # dict: dict = {"Requirement": None, "Total": None, "Complete": None, "Citations": None, "Implications": None,
    #     #               "Tests": None, "Exceptions": None, "TODOs": None}
    #     dict: dict = {"Requirement": None, "Total": None, "Incomplete": None}
    #     section: Section = Section()
    #     table: list[list] = [["MUST", 57, 27], ["SHOULD", 9, 8],
    #                          ["MAY", 5, 5]]
    # headers: list = ["Requirement", "Total", "Complete", "Citations", "Implications", "Tests", "Exceptions", "TODOs"]
    #     headers: list = ["Requirement", "Total", "Incomplete"]

    @staticmethod
    def _analyze_stats(section: Section) -> list[list]:
        """Given a section, return a table of analysis of section.

        Incomplete should be the only thing we care about during CI run.
        """
        section_analysis: list[dict] = []
        for level in RequirementLevel:
            total = [entry.level == level for entry in section.requirements.values()]
            completes = [entry.status == "complete" for entry in total]  # we don't care
            in_completes = [entry not in completes for entry in total]  # we do care
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
    def report_section(table: list[list]) -> bool:
        """Report Section stats."""
        print(tabulate(table, HEADERS, tablefmt="simple"))  # noqa: T001
        return True

# shoulds = [entry.level == RequirementLevel.SHOULD for entry in section.requirements.values()]
#
# entry.level == RequirementLevel.SHOULD
# for entry in section.requirements.values())
