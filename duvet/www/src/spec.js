// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

import { makeStyles } from "@material-ui/core/styles";
import { DataGrid } from "@mui/x-data-grid";
import Table from "@material-ui/core/Table";
import TableBody from "@material-ui/core/TableBody";
import TableCell from "@material-ui/core/TableCell";
import TableHead from "@material-ui/core/TableHead";
import TableRow from "@material-ui/core/TableRow";
import Tooltip from "@material-ui/core/Tooltip";
import { Link } from "./link";

export function Spec({ spec }) {
  return (
    <>
      <h2>{spec.title}</h2>

      <h3>Stats</h3>
      <Stats spec={spec} />

      <h3>Requirements</h3>
      <Requirements
        key={spec.id}
        requirements={spec.requirements}
        showSection
      />
    </>
  );
}

const useStyles = makeStyles((theme) => ({
  root: {
    "& > div": {
      // fix the weird inline style height
      height: "auto !important",
    },
  },
  text: {
    lineHeight: "initial !important",
    padding: theme.spacing(2, 1),
    whiteSpace: "normal !important",
    overflow: "auto !important",
  },
  error: {
    background: theme.palette.error.light,
    color: theme.palette.error.contrastText,
  },
  missingCitation: {
    background: theme.palette.warning.dark,
    color: theme.palette.warning.contrastText,
  },
  missingTest: {
    background: theme.palette.warning.light,
    color: theme.palette.warning.contrastText,
  },
  success: {
    background: theme.palette.success.light,
    color: theme.palette.success.contrastText,
  },
  info: {
    background: theme.palette.info.light,
    color: theme.palette.info.contrastText,
  },
}));

const LEVELS = ["MUST", "SHOULD", "MAY"];

const LEVEL_IDS = LEVELS.reduce((acc, level, idx) => {
  acc[level] = idx;
  return acc;
}, {});

export function Requirements({ requirements, showSection }) {
  const classes = useStyles();

  const columns = [];

  if (showSection) {
    columns.push({
      field: "section",
      headerName: "Section",
      valueGetter(params) {
        return params.row;
      },
      sortComparator(v1, v2) {
        return v1.cmp(v2);
      },
      renderCell(params) {
        const requirement = params.row;
        return (
          <Link
            to={{
              pathname: requirement.section.url,
              hash: `#A${requirement.id}`,
            }}
          >
            {requirement.section.shortId}
          </Link>
        );
      },
    });
  }

  columns.push(
    ...[
      {
        field: "level",
        headerName: "Requirement",
        width: 120,
        sortComparator(v1, v2) {
          return LEVEL_IDS[v2] - LEVEL_IDS[v1];
        },
      },
      {
        field: "status",
        headerName: "Status",
        width: 150,
        valueGetter(params) {
          return requirementStatus(params.row) || [];
        },
        valueFormatter(params) {
          return (params.value || requirementStatus(params.row))[1];
        },
        sortComparator(v1, v2) {
          return v1[0] - v2[0];
        },
        cellClassName(params) {
          const cls = (params.value || requirementStatus(params.row))[2];
          return classes[cls];
        },
      },
    ]
  );

  function listColumn(params) {
    columns.push({
      width: 150,
      sortComparator(v1, v2) {
        const a = v1.join(", ");
        const b = v2.join(", ");
        return a.localeCompare(b);
      },
      valueFormatter(params) {
        return (params.value || []).join(", ");
      },
      ...params,
    });
  }

  if (requirements.maxFeatures)
    listColumn({
      width: 200,
      field: "features",
      headerName: requirements.maxFeatures === 1 ? "Feature" : "Features",
    });

  if (requirements.maxTrackingIssues)
    listColumn({
      field: "tracking_issues",
      headerName:
        requirements.maxTrackingIssues === 1
          ? "Tracking Issue"
          : "Tracking Issues",
      renderCell(params) {
        return params.value.map((issue) =>
          issue.href ? (
            <Link key={issue.title} href={issue.href}>
              {issue.title}
            </Link>
          ) : (
            issue.title
          )
        );
      },
    });

  if (requirements.maxTags)
    listColumn({
      field: "tags",
      headerName: requirements.maxTags === 1 ? "Tag" : "Tags",
    });

  columns.push({
    field: "comment",
    headerName: "Text",
    sortable: false,
    width: 850,
    cellClassName: classes.text,
  });

  return (
    <div className={classes.root}>
      <DataGrid
        pageSize={25}
        disableSelectionOnClick
        autoHeight={true}
        rows={requirements}
        columns={columns}
        sortingOrder={["desc", "asc", null]}
      />
    </div>
  );
}

const useStatsStyles = makeStyles((theme) => ({
  totals: {
    borderTop: `2px solid ${theme.palette.grey.A200}`,
  },
}));

export function Stats({ spec: { stats } }) {
  const classes = useStatsStyles();

  return (
    <>
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell component="th">Requirement</TableCell>
            <TableCell align="right">Total</TableCell>
            <TableCell align="right">Complete</TableCell>
            <TableCell align="right">Citations</TableCell>
            <TableCell align="right">Implications</TableCell>
            <TableCell align="right">Tests</TableCell>
            <TableCell align="right">Exceptions</TableCell>
            <TableCell align="right">TODOs</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {LEVELS.filter((level) => stats[level].total).map((level) => (
            <StatsRow key={level} title={level} stats={stats[level]} />
          ))}
          <StatsRow
            className={classes.totals}
            title="Totals"
            stats={stats.overall}
          />
        </TableBody>
      </Table>
    </>
  );
}

function StatsRow({ title, stats, ...props }) {
  return (
    <TableRow {...props}>
      <TableCell component="th">{title}</TableCell>
      <TableCell align="right">{stats.total}</TableCell>
      {[
        "complete",
        "citations",
        "implications",
        "tests",
        "exceptions",
        "todos",
      ].map((name) => (
        <TableCell key={name} align="right">
          <Tooltip title={stats.percent(name)}>
            <span>{stats[name]}</span>
          </Tooltip>
        </TableCell>
      ))}
    </TableRow>
  );
}

function requirementStatus(requirement) {
  if (requirement.isComplete) return [1, "Complete", "success"];
  if (requirement.isOk) return [2, "Exception", "info"];
  if (requirement.spec === requirement.citation)
    return [4, "Missing test", "missingTest"];
  if (requirement.spec === requirement.test)
    return [5, "Missing citation", "missingCitation"];
  if (requirement.todo) return [7, "Not started", "error"];
  if (requirement.incomplete === requirement.spec)
    return [8, "Unknown", "error"];

  return [6, "Partial coverage", "warning"];
}
