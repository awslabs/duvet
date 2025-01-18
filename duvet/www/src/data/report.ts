import {
  type IReport,
  type ISpecification,
  type ISection,
  type ILine,
  type IAnnotation,
  type IRef,
  type ILevel,
  type IAnnotationType,
  type IStatus,
} from './raw-report';

export { ILevel as Level };

export class Report {
  public title: string;
  public specifications: IndexedMap<Specification> = new IndexedMap();
  public sections: Section[] = [];
  public annotations: Annotation[] = [];
  public refs: Ref[] = [];
  public requirements: Annotation[] = [];
  public references: Annotation[] = [];

  constructor(input: IReport) {
    this.title = input.title || 'Compliance Coverage Report';
    this.mapRefs(input);
    this.mapSpecifications(input);
    this.mapAnnotations(input);
    this.mapStats(input);
  }

  private mapRefs(input: IReport) {
    (input.refs || []).forEach((ref: IRef, id: number) => {
      const refObj = new Ref(id, ref);
      this.refs.push(refObj);
    });
  }

  private mapSpecifications(report: IReport) {
    Object.keys(report.specifications || {}).forEach((id) => {
      const input = report.specifications[id];
      const specification = new Specification(id, input, report, this);

      this.specifications.byIdx.push(specification);
      this.specifications.byId.set(id, specification);
      this.specifications.byId.set(encodeURIComponent(id), specification);

      // copy all of the requirements into the top level
      specification.sections.forEach((section) => this.sections.push(section));
    });
  }

  private mapAnnotations(input: IReport) {
    const blobLinker = createBlobLinker(input.blob_link);
    const newIssueLinker = createNewIssueLinker(input.issue_link);

    (input.annotations || []).forEach((anno: IAnnotation, id: number) => {
      const annotation = new Annotation(
        id,
        anno,
        blobLinker(anno),
        newIssueLinker,
        input,
        this,
      );
      this.annotations.push(annotation);
    });

    // resolve all of the related annotations
    this.annotations.forEach((anno) => {
      anno.related = (anno.status.related || []).map(
        (id) => this.annotations[id],
      );
    });
  }

  private mapStats(input: IReport) {
    const issueLinker = createIssueLinker(input.issue_link);

    this.specifications.forEach((spec: Specification) => {
      spec.resolveRequirements(this);

      spec.requirements.sort(sortAnnotation);

      spec.sections.forEach((section: Section) => {
        section.resolveRequirements(this);
        section.requirements.sort(sortAnnotation);
        section.stats = new RequirementStats(section.requirements, issueLinker);
        section.references.forEach((ref) => spec.references.push(ref));
      });

      spec.stats = new RequirementStats(spec.requirements, issueLinker);

      // copy all of the requirements into the top level
      spec.requirements.forEach((req) => this.requirements.push(req));
      spec.references.forEach((req) => this.references.push(req));
    });
  }
}

export class IndexedMap<T> {
  public byIdx: T[] = [];
  public byId: Map<string, T> = new Map();

  public forEach(cb: ((value: T) => void) | ((value: T, idx: number) => void)) {
    this.byIdx.forEach(cb);
  }

  public get length() {
    return this.byIdx.length;
  }
}

export class Ref implements IRef {
  public id: number;
  public citation: boolean;
  public exception: boolean;
  public implication: boolean;
  public level?: ILevel;
  public spec: boolean;
  public test: boolean;
  public todo: boolean;

  constructor(id: number, input: IRef) {
    this.id = id;
    this.level = input.level;
    this.citation = input.citation || false;
    this.exception = input.exception || false;
    this.implication = input.implication || false;
    this.spec = input.spec || false;
    this.test = input.test || false;
    this.todo = input.todo || false;
  }
}

export class Annotation {
  public id: number;
  public url: string;
  public specification: Specification;
  public section: Section;
  public source?: BlobLink;
  public target: string;
  public features: string[];
  public tracking_issues: string[];
  public type: IAnnotationType;
  public level?: ILevel;
  public newIssue: newIssueLinker;
  public status: IStatus;
  public text: string = '';
  public comment: string | null;
  public raw: IAnnotation;
  public related: Annotation[] = [];

  constructor(
    id: number,
    raw: IAnnotation,
    source: BlobLink,
    issueLinker: newIssueLinker,
    report: IReport,
    out: Report,
  ) {
    // TODO url

    this.raw = raw;
    this.status = (report.statuses || [])[id] || {};
    this.type = raw.type || 'CITATION';
    this.level = raw.level;
    this.comment = raw.comment;

    this.id = id;
    this.source = source;
    this.specification = out.specifications.byId.get(raw.target_path);
    this.section = this.specification.sections.byId.get(raw.target_section);

    // allow references to be wrong for the given section type for backward-compatibility
    if (!this.section) {
      let id = raw.target_section
        .replace(/^section-/, '')
        .replace(/^appendix-/, '');
      let sections = this.specification.sections;
      this.section =
        sections.byId.get(`section-${id}`) ||
        sections.byId.get(`appendix-${id}`);
    }

    this.url = `${this.section.url}&ref=${id}`;

    this.target = `${this.specification.id}#${this.section.id}`;
    this.newIssue = issueLinker;
  }

  get complete(): boolean {
    return (
      (this.spec === this.citation && this.spec === this.test) ||
      this.spec === this.implication
    );
  }

  get incomplete(): boolean {
    return !this.complete;
  }

  get ok(): boolean {
    return this.complete || this.exception == this.spec;
  }

  get citation(): boolean {
    return this.status.citation > 0;
  }

  get exception(): boolean {
    return this.status.exception > 0;
  }

  get implication(): boolean {
    return this.status.implication > 0;
  }

  get spec(): boolean {
    return this.status.spec > 0;
  }

  get test(): boolean {
    return this.status.test > 0;
  }

  get todo(): boolean {
    return this.status.todo > 0;
  }

  get canComment(): boolean {
    return this.exception || this.todo;
  }

  get allComments(): string {
    const comments = [];
    if (this.comment) comments.push(this.comment);

    this.related.forEach((anno) => {
      if (anno.comment) comments.push(anno.comment);
    });

    return comments.join('\n\n');
  }

  public cmp(b: Annotation): number {
    const a = this;
    if (a.specification === b.specification && a.section.idx !== b.section.idx)
      return a.section.idx - b.section.idx;
    return a.id - b.id;
  }
}

export class Specification {
  public id: string;
  public url: string;
  public format: string;
  public title: string;
  public sections: IndexedMap<Section> = new IndexedMap();
  public stats: RequirementStats = new RequirementStats([], null);
  public requirements: Annotation[] = [];
  public references: Annotation[] = [];
  public raw: ISpecification;

  constructor(
    id: string,
    input: ISpecification,
    inputRoot: IReport,
    report: Report,
  ) {
    this.id = id;
    const parts = id.split('/');
    this.title = input.title || parts[parts.length - 1].replace('.txt', '');
    this.url = `/spec/${encodeURIComponent(id)}`;
    this.format = input.format || 'ietf';
    this.raw = input;

    (input.sections || []).forEach((sec: ISection, idx: number) => {
      const section = new Section(idx, sec, this, inputRoot, report);

      this.sections.byIdx.push(section);
      this.sections.byId.set(section.id, section);
      this.sections.byId.set(encodeURIComponent(section.id), section);
    });
  }

  get isIetf() {
    return this.format == 'ietf';
  }

  get isMarkdown() {
    return this.format == 'markdown';
  }

  resolveRequirements(report: Report) {
    this.requirements = (this.raw.requirements || []).map(
      (id) => report.annotations[id],
    );

    this.sections.forEach((section) => {
      section.references.forEach((ref) => {
        this.references.push(ref);
      });
    });
  }
}

export class Section {
  public id: string;
  public shortId: string;
  public idx: number;
  public url: string;
  public title: string;
  public lines: Line[];
  public specification: Specification;
  public stats: RequirementStats = new RequirementStats([], null);
  public requirements: Annotation[] = [];
  public references: Annotation[] = [];
  public raw: ISection;

  constructor(
    idx: number,
    input: ISection,
    spec: Specification,
    inputRoot: IReport,
    report: Report,
  ) {
    this.id = input.id;
    this.idx = idx;
    this.title = input.title || '';
    this.url = `${spec.url}?section=${encodeURIComponent(input.id)}`;
    this.lines = (input.lines || []).map((line) => new Line(line, report));
    this.specification = spec;
    this.raw = input;

    this.requirements = (input.requirements || []).map(
      (id) => report.annotations[id],
    );
    this.shortId = spec.isIetf
      ? this.id.replace(/^section-/, '').replace(/^appendix-/, '')
      : this.id;

    // include the section id with the title for IETF documents
    if (spec.isIetf && !this.id.startsWith('name-')) {
      this.title = `${this.shortId}. ${this.title}`;
    }
  }

  resolveRequirements(report: Report) {
    this.requirements = (this.raw.requirements || []).map(
      (id) => report.annotations[id],
    );

    const references = new Set();

    this.lines.forEach((line) => {
      line.regions.forEach((region) => {
        region.annotations = region.annotationIdx.map((id) => {
          const anno = report.annotations[id];

          anno.related.forEach((related) => {
            if (references.has(related.id)) return;
            this.references.push(related);
            references.add(related.id);
          });

          // update the annotation's text
          anno.text = anno.text ? anno.text + ` ${region.text}` : region.text;

          return anno;
        });
      });
    });
  }
}

export class Line {
  public regions: LineRegion[] = [];

  constructor(line: ILine, report: Report) {
    if (typeof line === 'string') {
      this.regions.push(LineRegion.fromString(line));
      return;
    }

    this.regions = line.map((ref: string | [number[], number, string]) => {
      if (typeof ref === 'string') return LineRegion.fromString(ref);

      const [ids, status, text] = ref;
      return {
        annotations: [],
        annotationIdx: ids,
        status: report.refs[status] || report.refs[0],
        text,
      };
    });
  }
}

export class LineRegion {
  public annotations: Annotation[] = [];
  public annotationIdx: number[] = [];
  public status: Status = new Status();
  public text: string = '';

  static fromString(line: string): LineRegion {
    const region = new LineRegion();
    region.text = line;
    return region;
  }
}

export class Status {
  public annotation: boolean = false;
  public citation: boolean = false;
  public exception: boolean = false;
  public implication: boolean = false;
  public spec: boolean = false;
  public test: boolean = false;
  public todo: boolean = false;
}

export default function (input: any): Report {
  return new Report(input);
}

export type StatName =
  | 'total'
  | 'complete'
  | 'incomplete'
  | 'citations'
  | 'implications'
  | 'tests'
  | 'exceptions'
  | 'todos';

export class Stats {
  public total: number = 0;
  public complete: number = 0;
  public incomplete: number = 0;
  public citations: number = 0;
  public implications: number = 0;
  public tests: number = 0;
  public exceptions: number = 0;
  public todos: number = 0;

  onRequirement(requirement: Annotation) {
    this.total += 1;

    if (requirement.incomplete) this.incomplete += 1;
    else if (requirement.ok) this.complete += 1;

    if (requirement.citation) this.citations += 1;
    if (requirement.implication) this.implications += 1;
    if (requirement.test) this.tests += 1;
    if (requirement.exception) this.exceptions += 1;
    if (requirement.todo) this.todos += 1;
  }

  public percent(field: StatName): Percent {
    return new Percent(this[field], this.total);
  }
}

export class Percent {
  public value: number;
  public total: number;

  constructor(value: number, total: number) {
    this.total = total;
    this.value = value;
  }

  public get fraction() {
    return this.total ? this.value / this.total : 0;
  }

  public toNumber() {
    return this.fraction * 100;
  }

  public toString() {
    return Number(this.fraction).toLocaleString(undefined, {
      style: 'percent',
      minimumFractionDigits: 0,
      maximumFractionDigits: 2,
    });
  }
}

export class RequirementStats {
  public overall: Stats = new Stats();
  public MUST: Stats = new Stats();
  public SHOULD: Stats = new Stats();
  public MAY: Stats = new Stats();

  constructor(reqs: Annotation[], issueLinker) {
    reqs.maxFeatures = 0;
    reqs.maxTrackingIssues = 0;
    reqs.maxTags = 0;

    reqs.forEach((requirement) => {
      this.overall.onRequirement(requirement);
      let s = this[requirement.level] || new Stats();
      this[requirement.level] = s;
      s.onRequirement(requirement);
      const features = new Set();
      const tracking_issues = new Set();
      const tags = new Set();

      function onRelated(related) {
        if (related.feature) features.add(related.feature);
        if (related.tracking_issue) tracking_issues.add(related.tracking_issue);
        (related.tags || []).forEach(tags.add, tags);
      }

      onRelated(requirement);
      (requirement.related || []).forEach(onRelated);

      requirement.features = Array.from(features);
      requirement.features.sort();
      reqs.maxFeatures = Math.max(reqs.maxFeatures, features.size);

      requirement.tracking_issues = Array.from(tracking_issues);
      requirement.tracking_issues.sort();
      requirement.tracking_issues =
        requirement.tracking_issues.map(issueLinker);
      reqs.maxTrackingIssues = Math.max(
        reqs.maxTrackingIssues,
        tracking_issues.size,
      );

      requirement.tags = Array.from(tags);
      requirement.tags.sort();
      reqs.maxTags = Math.max(reqs.maxTags, tags.size);
    });
  }
}

function sortAnnotation(a: Annotation, b: Annotation) {
  return a.cmp(b);
}

export class BlobLink {
  public title: string;
  public href: string;

  constructor(title: string, href: string) {
    this.title = title;
    this.href = href;
  }

  public toString() {
    return this.title;
  }
}

function createBlobLinker(blob_link: string) {
  blob_link = (blob_link || '').replace(/\/+$/, '');

  return (anno: IAnnotation): BlobLink | null => {
    if (!anno.source) return null;

    let link = anno.source;

    if (anno.line > 0) {
      link += `#L${anno.line}`;
    }

    const href = blob_link.length ? `${blob_link}/${link}` : null;
    return new BlobLink(link, href);
  };
}

export class IssueLink {
  public title: string;
  public href: string;

  constructor(title: string, href: string) {
    this.title = title;
    this.href = href;
  }

  toString() {
    return this.title;
  }
}

function createIssueLinker(base: string) {
  base = (base || '').replace(/\/+$/, '');

  return (issue: string | undefined): IssueLink | null => {
    if (!issue) return null;

    if (/^http(s)?:/.test(issue)) return new IssueLink(issue, issue);

    const href = base.length ? `${base}/${issue}` : null;
    return new IssueLink(issue, href);
  };
}

export type newIssueLinker = () => string | false;

function createNewIssueLinker(base: string): newIssueLinker {
  base = (base || '').replace(/\/+$/, '');

  return function () {
    if (!base) return false;
    if (!this.comment) return false;

    const url = new URL(`${base}/new`);

    const quote = this.comment
      .trim()
      .split('\n')
      .map((line) => `> ${line}`)
      .join('\n');

    const body = `
From [${this.section.title}](${this.target}) in [${this.specification.title}](${this.target_path}):

${quote}`;

    url.searchParams.set('body', body);
    const labels = [
      'compliance',
      this.level && `compliance:${this.level}`,
      this.specification.title && `spec:${this.specification.title}`,
    ]
      .concat(this.features)
      .filter((l) => !!l)
      .join(',');
    url.searchParams.set('labels', labels);

    return url.toString();
  };
}
