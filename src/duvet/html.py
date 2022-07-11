# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Html generator used by duvet-python."""
import json
import pathlib

import attr
import click
from attrs import define, field

from duvet.json_report import JSONReport

DEFAULT_HTML_PATH = "duvet-report.html"
DEFAULT_JSON_PATH = "duvet-result.json"

# Backport of Python standard library importlib.resources module for older Pythons
from importlib_resources import files


@define
class HTMLReport:
    """Container of the HTML report."""

    json_report: JSONReport = field(init=False)
    data: dict = field(init=False, default=attr.Factory(dict))

    def from_json(self, json_path=DEFAULT_JSON_PATH):
        """Parse fata from JSON file."""
        with open(json_path, "r+", encoding="utf-8") as json_file:
            self.data = json.load(json_file)

    def write_html(self, html_path=DEFAULT_HTML_PATH) -> str:
        """Write HTML report."""
        template_string = files("duvet").joinpath("index.html").read_text()

        # Get HTML head before JSON.
        html_head_end = template_string.find("</head>")
        html_head = template_string[:html_head_end]

        # Get HTML string between JSON and JS
        html_body_end = template_string.find("</body>")
        html_between_json_and_js = template_string[html_head_end:html_body_end]

        # Get HTML string after JS
        html_end = template_string[html_body_end:]

        # Create JSON string.
        json_string = f"""<script id="result" type="application/json">{json.dumps(self.data)}</script>"""

        # Create JavaScript string.
        js_string = f"""<script>{files("duvet").joinpath("script.js").read_text()}</script>"""

        # Create HTML string and write to new HTML file.
        html_string = "\n".join([html_head, json_string, html_between_json_and_js, js_string, html_end])
        with open(html_path, "w+", encoding="utf-8") as html_file:
            html_file.write(html_string)

        # Return HTML path
        full_html_path = str(pathlib.Path(html_path).resolve())
        return full_html_path

# //= compliance/duvet-specification.txt#2.6.3
# //= type=implication
# //# It MUST have all a link for each included specifications.

# //= compliance/duvet-specification.txt#2.6.3
# //= type=implication
# //# For each link it MUST have a table summarizing the matrix of requirements
# //# crossed annotation types, and include totals for both sides.

# //= compliance/duvet-specification.txt#2.6.3
# //= type=implication
# //# It MUST have all a link for annotations that do not match any included specifications.

# //= compliance/duvet-specification.txt#2.6.3
# //= type=TODO
# //# It MUST have all a link for annotations not associated with any specifications.

# //= compliance/duvet-specification.txt#2.6.4
# //= type=implication
# //# It MUST have a table summarizing the matrix of requirements
# //# across annotation types, and include totals for both sides.

# //= compliance/duvet-specification.txt#2.6.4
# //= type=implication
# //# It MUST show a table with a row for each requirement.

# //= compliance/duvet-specification.txt#2.6.4
# //= type=implication
# //# The table MUST have a column for:

# //= compliance/duvet-specification.txt#2.6.5
# //= type=TODO
# //# It MUST show all text from the section.

# //= compliance/duvet-specification.txt#2.6.5
# //= type=implication
# //# The table MUST have a column for:

# //= compliance/duvet-specification.txt#2.6.5
# //= type=implication
# //# It MUST highlight the text for every requirement.

# //= compliance/duvet-specification.txt#2.6.5
# //= type=implication
# //# It MUST highlight the text that matches any annotation.

# //= compliance/duvet-specification.txt#2.6.5
# //= type=implication
# //# Any highlighted text MUST have a mouse over that shows its annotation information.




