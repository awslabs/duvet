# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Html generator used by duvet-python."""
import json
import pathlib
from importlib.resources import files

import attr
import click
from attrs import define, field

from duvet.identifiers import DEFAULT_HTML_PATH, DEFAULT_JSON_PATH
from duvet.json_report import JSONReport


@define
class HTMLReport:
    """Container of the HTML report."""

    data: dict = field(init=True, default=attr.Factory(dict))

    @classmethod
    def from_json_file(cls, json_path=DEFAULT_JSON_PATH):
        """Load data from JSON file."""
        with open(json_path, "r+", encoding="utf-8") as json_file:
            cls.data = json.load(json_file)
            return cls()

    @classmethod
    def from_json_report(cls, json_report: JSONReport):
        """Load data from JSONReport."""
        cls.data = json_report.get_dictionary()
        return cls()

    def write_html(self, html_path=DEFAULT_HTML_PATH) -> str:
        """Write HTML report."""
        template_string = files("duvet").joinpath("index.html").read_text(encoding="utf-8")

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
        js_string = f"""<script>{files("duvet").joinpath("script.js").read_text(encoding="utf-8")}</script>"""

        # Create HTML string and write to new HTML file.
        html_string = "\n".join([html_head, json_string, html_between_json_and_js, js_string, html_end])
        with open(html_path, "w+", encoding="utf-8") as html_file:
            html_file.write(html_string)

        # Return HTML path
        full_html_path = str(pathlib.Path(html_path).resolve())
        click.echo(f"""Write HTML report to {full_html_path}""")
        return full_html_path