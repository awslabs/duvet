#!/usr/bin/env node
// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

/* This is some sugar to produce compliance TOML
 * from existing markdown.
 * It uses `cargo-compliance`, `xml2rfc` and `kramdown`.
 *
 */

const { extname, basename, resolve, dirname, join, relative } = require("path");
const { execSync } = require("child_process");
const {
  readFileSync,
  writeFileSync,
  statSync,
  constants,
  mkdirSync,
} = require("fs");
const ext = ".md";
const pathToComplianceRoot = `${relative(process.cwd(), `${__dirname}/../compliance`)}`;

needs(
  () => execSync("which kramdown-rfc2629"),
  "kramdown-rfc2629 needs to be installed, try `gem install kramdown-rfc2629 -v 1.5.21`"
);
needs(
  () => execSync("which xml2rfc"),
  "xml2rfc needs to be installed, try `pip install xml2rfc==3.5.0`"
);
needs(
  () => execSync("which duvet"),
  "duvet needs to be installed, try `util/install-duvet`"
);

/* May need to change this to a better parser...
 * When run with a shebang
 * the argv list will start,
 * with the path to node
 * then the path to this script.
 * So the 3rd element ([2])
 * is the first user parameter.
 */
process.argv.slice(2).map(extract_needs).map(extract);

function extract(filePath) {
  const fileName = basename(filePath, ext);
  const tmpdir = require("os").tmpdir();
  const markdownSpecFile = resolve(tmpdir, `${fileName}${ext}`);
  const xmlRfcFile = resolve(tmpdir, `${fileName}.xml`);
  const complianceSpec = join(pathToComplianceRoot, dirname(filePath), `${fileName}.txt`);
  const complianceDir = join(pathToComplianceRoot, dirname(filePath), fileName);

  // Create the root compliance directory if it doesn't exist
  try {
    statSync(pathToComplianceRoot).isDirectory();
  } catch (ex) {
    mkdirSync(pathToComplianceRoot, { recursive: true });
  }

  /*
    1. Get the file name without extension
    2. Add the RFC crap to a new tmp file
    3. kramdown
    4. xml2rfc
    5. cargo-compliance extract
  */

  // Write the spec file with the header and footer
  writeFileSync(
    markdownSpecFile,
    [header(fileName), readFileSync(filePath, { encoding: "utf8" }), footer()].join("\n"),
    { encoding: "utf8" }
  );

  // Convert the markdown file from RFC XML
  execSync(["kramdown-rfc2629", "-3", markdownSpecFile, ">", xmlRfcFile].join(" "), {stdio: 'inherit'});

  // An existing spec may exists, clean up first
  try {
    // This will throw if the directory does not exist
    statSync(complianceDir).isDirectory();
    // If the directory exists, remove it. Nothing could go wrong... :(
    execSync(["rm", "-rf", complianceDir].join(" "));
  } catch (ex) {
    // If the directory does not exist, that is OK
    needs(ex.errno === -2, "Unknown error");
  }

  // make sure the compliance directory exists
  mkdirSync(complianceDir, { recursive: true });

  // Convert the RFC XML to a ietf rfc
  execSync(["xml2rfc",
    "-P", xmlRfcFile,
    "-s", "'Too long line found'", // Suppress warnings about long table line length
    "-s", "'Total table width'",   // Suppress warnings about overall table width
    "-o", complianceSpec
  ].join(" "), {stdio: 'inherit'});

  const args = ["duvet", "extract", `${complianceSpec}`, "-o", "compliance"];
  // extract the specification
  execSync(args.join(" "), { encoding: 'utf8', stdio: 'inherit'});
}

function extract_needs(filePath) {
  needs(extname(filePath) === ext, `Unsupported ext ${ext}`);
  needs(
    () => statSync(filePath).isFile(),
    `Specification file ${filePath} does not exist.`
  );
  return filePath;
}

// The `ipr: none` is particularly important
// this removed boilerplate (especially Copyright)
function header(docName) {
  return `---
title: ${docName}
abbrev: ${docName}
docname: ${docName}
category: info
ipr: none
area: General
workgroup: AWS Crypto Tools
keyword: INTERNAL-ONLY
stand_alone: yes
pi: [toc, sortrefs, symrefs]
author:
  -
    ins: Amazon AWS
    name: Amazon AWS
    organization: Amazon AWS
    email: cryptools+rfc@amazon.com
normative:
  RFC2119:
informative:
--- abstract
The ${docName} specification for the AWS Encryption SDK.
--- middle
# Conventions and Definitions
The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD",
"SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and "OPTIONAL" in this
document are to be interpreted as described in BCP 14 {{RFC2119}} {{!RFC8174}}
when, and only when, they appear in all capitals, as shown here
  `;
}

function footer() {
  return `--- back
# Acknowledgments
{:numbered="false"}
`;
}

function needs(condition, errorMessage) {
  if (typeof condition === "function") {
    try {
      needs(condition(), errorMessage);
    } catch (ex) {
      throw new Error(errorMessage);
    }
  }

  if (!condition) {
    throw new Error(errorMessage);
  }
}
