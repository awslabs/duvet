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
        with open(html_path, "w+", encoding="utf-8") as html_file:
            # Write header.
            html_file.write("<!DOCTYPE html>\n")
            html_file.write("<html>\n")
            html_file.write("<head>\n")
            html_file.write('<meta charset="utf-8">\n')
            html_file.write("<title>\n")
            html_file.write("Compliance Coverage Report\n")
            html_file.write("</title>\n")

            # Write JSON.
            html_file.write('<script type="application/json" id=result>\n')
            html_file.write(json.dumps(self.data))
            html_file.write("\n")
            html_file.write("</script>\n")
            html_file.write("</head>\n")
            html_file.write("<body>\n")
            html_file.write("<div id=root></div>\n")

            # Write JavaScript.
            html_file.write("<script>\n")
            with open("../../www/public/script.js", "r", encoding="utf-8") as javascript:
                html_file.write(javascript.read())
            html_file.write("</script>\n")
            html_file.write("</body>\n")
            html_file.write("</html>\n")

        # Open file in browser.
        webbrowser.open(html_path)


html_report = HTMLReport()
html_report.from_json()
html_report.write_html()
