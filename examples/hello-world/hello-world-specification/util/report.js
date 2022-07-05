#!/usr/bin/env node

const { relative, join, extname } = require("path");
const { execSync } = require("child_process");

needs(
  () => execSync("which duvet"),
  "duvet needs to be installed try `util/install-duvet`"
);

const data = execSync("git remote -v")
  .toString()
  .split("\n")
  /* Push because that is where the code can go */
  .filter((line) => line.includes("(fetch)"))
  /* Trim the identifier by removing the name and the (push) */
  .map((line) => line.split(/\s/)[1])
  /* Only git or https please */
  .filter((line) => line.startsWith("https://") || line.startsWith("git@github.com:"))
  /* Convert git to https urls */
  .map((line) =>
    line.startsWith("https://")
      ? line
      : line.replace("git@github.com:", "https://github.com/")
  )
  /* Drop the `.git` because this is not in GitHub blob or issue urls */
  .map((line) => line.replace(".git", ""))
  /* aws or awslabs only, no forks */
  .filter((line) => line.startsWith("https://github.com/aws"));

needs(data.length, "Not in a git sandbox?");
needs(data.length === 1, `Ambiguous urls ${JSON.stringify(data)}`);
const gitHubUrl = new Set(data).values().next().value;

/* cargo-compliance need to have all the specification paths line up.
 * The extracted paths are relative to the specification repo,
 * but when running the report the specification repo is a sub-module.
 * To make this work, I run cargo-compliance in the specification repo,
 * but then output to the this command was run
 * (the root of the implementation repo).
 * A little work also needs to be done on the source files as well.
 */
const specificationDirectory = relative(process.cwd(), `${__dirname}/..`);
const relativePath = relative(specificationDirectory, process.cwd());

/* May need to change this to a better parser...
 * When run with a shebang
 * the argv list will start,
 * with the path to node
 * then the path to this script.
 * So the 3rd element ([2])
 * is the first user parameter.
 */
const sourcePatterns = process.argv
  .slice(2)
  // Only support relative paths
  .map((pattern) => join(relativePath, relative(process.cwd(), pattern)))
  /* Python files use # for comments
   * Therefore //= and //# can not work
   * to identify the compliance sections.
   * The compliance tool lets you override
   * the meta and content patterns.
   * So lets make it easy for Python.
   */
  .map((pattern) => (extname(pattern) === ".py" ? `(# //=,# //#)${pattern}` : pattern))
  .map((pattern) => `--source-pattern '${pattern}'`);
needs(sourcePatterns.length, "No source patterns");

const args = [
  "duvet",
  "report ",
  "--ci",
  `--spec-pattern "compliance/**/*.toml" `,
  ...sourcePatterns,
  "--require-citations true ",
  "--require-tests true ",
  `--blob-link "${gitHubUrl}/blob/$BLOB" `,
  `--issue-link "${gitHubUrl}/issues" `,
  "--no-cargo ",
  `--html ${join(relativePath, "specification_compliance_report.html")}`,
];

try {
  const out = execSync(args.join(" "), {
    encoding: "utf8",
    cwd: specificationDirectory,
  });
  console.log(out);
} catch (ex) {
  const { stdout, status } = ex;
  if (stdout) console.log(stdout);
  process.exit(status);
}

function needs(condition, errorMessage) {
  if (!condition) {
    throw new Error(errorMessage);
  }
}