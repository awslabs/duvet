# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Html generator used by duvet-python."""
import sys
import webbrowser
import json
from attrs import define
from domonic import domonic, script, link, style, html, head, body, div, render, article, h1

f = open("./view/result.json")
data = json.load(f)


with open("duvet-report.html","w+", encoding="utf-8") as html_file:
    html_file.write("<!DOCTYPE html>\n")
    html_file.write("<html>\n")
    html_file.write("<head>\n")
    html_file.write("<meta charset=\"utf-8\">\n")
    html_file.write("<title>\n")
    html_file.write("Compliance Coverage Report\n")
    html_file.write("</title>\n")

    html_file.write("<script type=\"application/json\" id=result>\n")
    html_file.write(json.dumps(data))
    html_file.write("\n")
    html_file.write("</script>\n")
    html_file.write("</head>\n")
    html_file.write("<body>\n")
    html_file.write("<div id=root></div>\n")
    html_file.write("<script>\n")
    with open("../../www/public/script.js","r", encoding="utf-8") as javascript:
        html_file.write(javascript.read())
    html_file.write("</script>\n")
    html_file.write("</body>\n")
    html_file.write("</html>\n")

webbrowser.open("duvet-report.html")
