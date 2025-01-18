import {
  useState,
  useMemo,
  useRef,
  useEffect,
  useCallback,
  default as React,
} from 'react';
import clsx from 'clsx';
import copyToClipboard from 'copy-to-clipboard';
import { Requirements } from './Requirements';
import { Link } from './Link';
import { useSection } from './Params';
import {
  Specification as ISpecification,
  Section as ISection,
  Line as ILine,
  LineRegion as ILineRegion,
} from '../data/report';

export interface IProps {
  section: ISection;
}

export const Section: React.FC<IProps> = ({ section }) => (
  <>
    <pre>
      <Title section={section} />
      {section.lines.map((line, i) => (
        <Line content={line} key={i} />
      ))}
    </pre>
  </>
);

export function Scroller({}) {
  const paramSection = useSection() || { url: null };

  const checkRoute = () => {
    const element = document.getElementById(paramSection.url);
    if (!element) return;
    element.scrollIntoView();
  };

  useEffect(checkRoute, [paramSection.url]);
}

function Title({ section }) {
  return (
    <div className="relative">
      <h2
        id={section.url}
        className={clsx(
          'font-semibold mt-8 text-xl',
          "hover:before:content-['#'] hover:before:absolute hover:before:-left-4",
          'hover:before:overflow-visible hover:before:text-right hover:before:opacity-50',
        )}
      >
        <Link to={section.url}>{section.title}</Link>
      </h2>
    </div>
  );
}

function Line({ content }: { content: ILine }) {
  return (
    <>
      {content.regions.map((reference, i) => {
        return <Region reference={reference} key={i} />;
      })}
      <br />
    </>
  );
}

function Region({ reference }: { reference: ILineRegion }) {
  if (!reference.annotations.length) return reference.text;

  const statusClass = regionStatus(reference);

  return (
    <span
      className={clsx(
        statusClass,
        'border-b-2',
        {
          ok: 'border-b-green-700',
          missingCitation: 'border-b-orange-500',
          missingTest: 'border-b-orange-500',
          error: 'border-b-red-600',
          exception: 'border-b-gray-800 dark:border-b-gray-400',
          neutral: 'border-b-blue-700',
        }[statusClass],
      )}
    >
      {reference.text}
    </span>
  );
}

function regionStatus({ status }: ILineRegion): string {
  let statusClass = 'neutral';

  if (status.spec) {
    if (status.citation && status.test) {
      statusClass = 'ok';
    } else if (status.implication) {
      statusClass = 'ok';
    } else if (status.citation) {
      statusClass = 'missingTest';
    } else if (status.test) {
      statusClass = 'missingCitation';
    } else {
      statusClass = 'error';
    }
  }

  if (status.exception) {
    statusClass = 'exception';
  }

  return statusClass;
}

// function Quote({ reference }) {
//   const { status, text } = reference;
//   const classes = useStyles();
//   const [open, setOpen] = useState(false);
//   const selectedAnnotations = useAnnotationSelection();

//   const handleOpen = () => {
//     setOpen(true);
//   };
//   const handleClose = () => {
//     setOpen(false);
//   };

//   let statusClass = "neutral";

//   if (status.spec) {
//     if (status.citation && status.test) {
//       statusClass = "ok";
//     } else if (status.implication) {
//       statusClass = "ok";
//     } else if (status.citation) {
//       statusClass = "missingTest";
//     } else if (status.test) {
//       statusClass = "missingCitation";
//     } else {
//       statusClass = "error";
//     }
//   }

//   if (status.exception) {
//     statusClass = "exception";
//   }

//   let selected =
//     selectedAnnotations.size &&
//     reference.annotations.find((anno) => selectedAnnotations.has(anno.id));

//   return (
//     <>
//       <QuoteTooltip title={<Annotations reference={reference} />}>
//         <span
//           className={clsx(classes.reference, classes[statusClass], {
//             [classes.selected]: selected,
//           })}
//           onClick={handleOpen}
//         >
//           {text}
//         </span>
//       </QuoteTooltip>
//       <Dialog open={open} onClose={handleClose} maxWidth={false}>
//         <Paper className={classes.paper}>
//           <Annotations reference={reference} expanded />
//         </Paper>
//       </Dialog>
//     </>
//   );
// }

// function Annotations({ reference: { annotations, status }, expanded }) {
//   const refs = {
//     CITATION: [],
//     IMPLICATION: [],
//     SPEC: [],
//     TEST: [],
//     EXCEPTION: [],
//     TODO: [],
//     features: new Set(),
//     tracking_issues: new Set(),
//     tags: new Set(),
//   };

//   annotations.forEach((anno) => {
//     if (anno.source) {
//       (refs[anno.type || "CITATION"] || []).push(anno);
//     }
//     anno.features.forEach(refs.features.add, refs.features);
//     anno.tracking_issues.forEach(
//       refs.tracking_issues.add,
//       refs.tracking_issues
//     );
//     anno.tags.forEach(refs.tags.add, refs.tags);
//   });

//   const requirement = status.level ? <h3>Level: {status.level}</h3> : null;
//   const isOk = !!refs.SPEC.find((ref) => ref.isOk);
//   const showMissing = requirement && !isOk;

//   const comments = expanded ? refs.SPEC.filter((ref) => ref.comment) : [];

//   return (
//     <>
//       {requirement}
//       {comments.map((anno, i) => (
//         <Comment annotation={anno} key={anno.id} />
//       ))}
//       {expanded ? (
//         <AnnotationList title="Features" items={refs.features} />
//       ) : null}
//       {expanded ? (
//         <AnnotationList title="Tracking issues" items={refs.tracking_issues} />
//       ) : null}
//       {expanded ? <AnnotationList title="Tags" items={refs.tags} /> : null}
//       <AnnotationRef
//         title="Specifications"
//         refs={refs.SPEC.length > 1 ? refs.SPEC : []}
//       />
//       <AnnotationRef
//         title="Citations"
//         alt={showMissing && "Missing!"}
//         refs={refs.CITATION}
//       />
//       <AnnotationRef
//         title="Tests"
//         alt={showMissing && "Missing!"}
//         refs={refs.TEST}
//       />
//       <AnnotationRef title="Implications" refs={refs.IMPLICATION} />
//       <AnnotationRef
//         title="Exceptions"
//         refs={refs.EXCEPTION}
//         expanded={expanded}
//       />
//       <AnnotationRef title="TODOs" refs={refs.TODO} />
//     </>
//   );
// }

// const listItemStyle = { padding: "0 8px", display: "block" };

// function AnnotationList({ title, items }) {
//   if (!items.size) return null;
//   items = Array.from(items);

//   if (items.length === 1) {
//     // remove `s` if there's only 1
//     title = title.slice(0, title.length - 1);
//   } else {
//     // sort the items if there's more than 1
//     items.sort();
//   }

//   const content = items.map((item, idx) => {
//     const text = item.toString();
//     const content = item.href ? <Link href={item.href}>{text}</Link> : text;
//     return (
//       <ListItem style={{ ...listItemStyle, display: "inline" }} key={idx}>
//         {idx ? ", " : ""}
//         {content}
//       </ListItem>
//     );
//   });

//   return (
//     <div>
//       <h3 style={{ lineHeight: 1, display: "inline" }}>{title}</h3>
//       <List style={{ padding: 0, display: "inline" }}>{content}</List>
//     </div>
//   );
// }

// function AnnotationRef({ title, alt, refs, expanded }) {
//   if (!refs.length && !alt) {
//     return null;
//   }

//   const content = refs.length ? (
//     refs.map((anno, id) => {
//       const text = <ListItemText secondary={anno.source.title} />;
//       const content = anno.source.href ? (
//         <Link href={anno.source.href}>{text}</Link>
//       ) : (
//         text
//       );
//       return (
//         <ListItem style={listItemStyle} key={id}>
//           {content}
//           {expanded && anno.comment ? (
//             <Typography
//               variant="body2"
//               style={{ paddingLeft: 16, maxWidth: 500 }}
//             >
//               {anno.comment}
//             </Typography>
//           ) : null}
//         </ListItem>
//       );
//     })
//   ) : (
//     <ListItem style={listItemStyle}>
//       <Box color="error.main">
//         <h4>{alt}</h4>
//       </Box>
//     </ListItem>
//   );

//   return (
//     <>
//       <h4 style={{ lineHeight: 1 }}>{title}</h4>
//       <List style={{ padding: 0 }}>{content}</List>
//     </>
//   );
// }

// const useCommentStyles = makeStyles((theme) => ({
//   cite: {
//     display: "flex",
//     flexWrap: "wrap",
//     justifyContent: "right",
//     marginBottom: "2em",
//   },
// }));

// function Comment({ annotation }) {
//   const classes = useCommentStyles();
//   const [format, setFormat] = useState("comment");

//   const formatComment = {
//     toml: formatTomlComment,
//     comment: formatReferenceComment,
//   }[format];

//   const newIssueLink = annotation.newIssueLink();

//   return (
//     <>
//       <p>
//         <pre>{annotation.comment}</pre>
//       </p>
//       <div className={classes.cite}>
//         <Select
//           value={format}
//           onChange={(event) => setFormat(event.target.value)}
//           autoWidth
//         >
//           <MenuItem value={"comment"}>Comment</MenuItem>
//           <MenuItem value={"toml"}>Toml</MenuItem>
//         </Select>
//         <ButtonGroup size="small" color="primary" variant="contained">
//           {[
//             { label: "Citation", type: "citation" },
//             { label: "Implication", type: "implication" },
//             { label: "Test", type: "test" },
//             { label: "Exception", type: "exception" },
//             { label: "TODO", type: "TODO" },
//           ].map(({ label, type }) => (
//             <Cite
//               key={label}
//               getData={() => formatComment({ annotation, type })}
//               label={label}
//             />
//           ))}
//           {newIssueLink && (
//             <Button href={newIssueLink} target="_blank">
//               Issue
//             </Button>
//           )}
//         </ButtonGroup>
//       </div>
//     </>
//   );
// }

// function Cite({ getData, label, ...props }) {
//   const [copied, setCopied] = useState(false);

//   const onClick = () => {
//     copyToClipboard(getData());
//     setCopied(true);
//     setTimeout(() => setCopied(false), 1000);
//   };

//   return (
//     <Button onClick={onClick} {...props}>
//       {copied ? `${label} - Copied!` : label}
//     </Button>
//   );
// }

// function formatReferenceComment({ annotation, type }) {
//   let comment = [];
//   comment.push(`//= ${annotation.target}`);
//   if (type !== "citation") comment.push(`//= type=${type}`);
//   annotation.comment
//     .trim()
//     .split("\n")
//     .forEach((line) => {
//       comment.push(`//# ${line}`);
//     });
//   return comment.join("\n") + "\n";
// }

// function formatTomlComment({ annotation, type }) {
//   let comment = [`[[${type}]]`];
//   comment.push(`target = ${JSON.stringify(annotation.target)}`);
//   comment.push("quote = '''");
//   comment.push(...annotation.comment.trim().split("\n"));
//   comment.push("'''");
//   if (type === "exception") {
//     comment.push("reason = '''");
//     comment.push("TODO: Add reason for exception here");
//     comment.push("'''");
//   }

//   return comment.join("\n") + "\n";
// }
