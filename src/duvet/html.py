# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Html generator used by duvet-python."""
import json
import webbrowser

import attr
from attrs import define, field

from duvet.json_report import JSONReport

DEFAULT_HTML_PATH = "duvet-report.html"
DEFAULT_JSON_PATH = "duvet-result.json"

HTML_HEADER = """<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="description" content="A code quality tool to help bound correctness.">
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Duvet Compliance Coverage Report</title>
"""


@define
class HTMLReport:
    """Container of the HTML report."""

    json_report: JSONReport = field(init=False)
    data: dict = field(init=False, default=attr.Factory(dict))

    def from_json(self, json_path=DEFAULT_JSON_PATH):
        """Parse fata from JSON file."""
        with open(json_path, "r+", encoding="utf-8") as json_file:
            self.data = json.load(json_file)

    def write_html(self, html_path=DEFAULT_HTML_PATH):
        """Write HTML report."""
        with open("../../www/public/index.html", "r+", encoding="utf-8") as template:
            # Get HTML head.
            template_string = template.read()
        # Before JSON
        html_head_end = template_string.find("</head>")
        html_head = template_string[:html_head_end]
        # Between JSON and JS
        html_body_end = template_string.find("</body>")
        html_between_json_and_js = template_string[html_head_end:html_body_end]
        # After JS
        html_end = template_string[html_body_end:]

        with open(html_path, "w+", encoding="utf-8") as html_file:
            # Create JSON string.
            json_string = f"""
             <script id="result" type="application/json">
             {json.dumps(self.data)}
             </script>
            """

        # Create JavaScript string.
        with open("../../www/public/script.js", "r", encoding="utf-8") as javascript_file:
            js_string = f"""<script>
            {javascript_file.read()}
            </script>
            """

        html_string = "\n".join([html_head, json_string, html_between_json_and_js, json_string, html_end])

        with open(html_path, "w+", encoding="utf-8") as html_file:
            html_file.write(html_string)

        # Open file in browser.
        webbrowser.open(html_path)


html_report = HTMLReport()
html_report.from_json()
html_report.write_html()
