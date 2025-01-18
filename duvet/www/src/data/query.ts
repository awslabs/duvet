import { Report, Level, Specification, Section, Annotation } from './report';

export enum Mode {
  SPEC = 'Specification',
  SECTION = 'Section',
  REQUIREMENT = 'Requirement',
}

// Can either be set to an explicit value or wildcard match with null
export type Flag = boolean | null;

export class RangeInclusive {
  min: number;
  max: number;

  constructor([min, max]: [number, number]) {
    this.min = min;
    this.max = max;
  }

  contains(value: number): boolean {
    return value >= this.min && value <= this.max;
  }

  toString(): string {
    return `${this.min}-${this.max}`;
  }
}

export class Query {
  mode: Mode = Mode.SPEC;
  // Filter by specification source
  specifications: Set<string> = new Set();
  // Filter by level
  level: Set<Level> = new Set();
  // used for Specification and Section queries
  hasRequirements: Flag = true; // set to true by default to hide specs without requirements
  completeRange: RangeInclusive = new RangeInclusive([0, 100]);
  // use for Requirement queries
  isComplete: Flag = null;
  isOk: Flag = null;
  isImplementation: Flag = null;
  isImplication: Flag = null;
  isException: Flag = null;
  isTest: Flag = null;
  isTodo: Flag = null;

  withSpecifications(value: Set<string>): Query {
    const out = this.clone();
    out.specifications = value;
    return out;
  }

  withCompleteRange(value: [number, number] | RangeInclusive): Query {
    const out = this.clone();
    out.completeRange = Array.isArray(value)
      ? new RangeInclusive(value)
      : value;
    return out;
  }

  withMode(value: Mode): Query {
    const out = this.clone();
    out.mode = value;
    return out;
  }

  withLevel(value: Set<Level>): Query {
    const out = this.clone();
    out.level = value;
    return out;
  }

  withHasRequirements(value: Flag): Query {
    const out = this.clone();
    out.hasRequirements = value;
    return out;
  }

  withIsComplete(value: Flag): Query {
    const out = this.clone();
    out.isComplete = value;
    return out;
  }

  withIsOk(value: Flag): Query {
    const out = this.clone();
    out.isOk = value;
    return out;
  }

  withIsImplementation(value: Flag): Query {
    const out = this.clone();
    out.isImplementation = value;
    return out;
  }

  withIsImplication(value: Flag): Query {
    const out = this.clone();
    out.isImplication = value;
    return out;
  }

  withIsException(value: Flag): Query {
    const out = this.clone();
    out.isException = value;
    return out;
  }

  withIsTest(value: Flag): Query {
    const out = this.clone();
    out.isTest = value;
    return out;
  }

  withIsTodo(value: Flag): Query {
    const out = this.clone();
    out.isTodo = value;
    return out;
  }

  clone(): Query {
    const out = new Query();
    Object.assign(out, this);
    return out;
  }

  searchSpecs<T>(
    report: Report,
    map: (spec: Specification, idx: number) => T,
  ): T[] {
    const out = [];

    report.specifications.forEach((spec: Specification, idx) => {
      if (!this.matchSpec(spec)) return;
      out.push(map(spec, idx));
    });

    return out;
  }

  matchSpec(spec: Specification): boolean {
    if (this.specifications.size) {
      if (!this.specifications.has(spec.id)) return false;
    }

    if (typeof this.hasRequirements == 'boolean') {
      if (!!spec.requirements.length != this.hasRequirements) return false;
    }

    const percent = spec.stats.overall.percent('complete').toNumber();
    if (!this.completeRange.contains(percent)) return false;

    return true;
  }

  searchSections<T>(
    report: Report,
    map: (section: Section, idx: number) => T,
  ): T[] {
    const out = [];
    let idx = 0;

    report.specifications.byId.forEach((spec) => {
      spec.sections.byId.forEach((section) => {
        let i = idx;
        idx += 1;
        if (!this.matchSection(section)) return;
        out.push(map(section, i));
      });
    });

    return out;
  }

  matchSection(section: Section): boolean {
    if (typeof this.hasRequirements == 'boolean') {
      if (!!section.requirements.length != this.hasRequirements) return false;
    }

    if (this.specifications.size) {
      if (!this.specifications.has(section.specification.id)) return false;
    }

    if (this.level.size) {
      for (let level of this.level) {
        if (!section.stats[level].total) return false;
      }
    }

    const percent = section.stats.overall.percent('complete').toNumber();
    if (!this.completeRange.contains(percent)) return false;

    return true;
  }

  searchRequirements<T>(
    report: Report,
    map: (section: Annotation, idx: number) => T,
  ): T[] {
    const out = [];
    let idx = 0;

    report.requirements.forEach((requirement, idx) => {
      if (!this.matchRequirement(requirement)) return;
      out.push(map(requirement, idx));
    });

    return out;
  }

  matchRequirement(requirement: Annotation): boolean {
    if (!flagQuery(this.isComplete, requirement.complete)) return false;
    if (!flagQuery(this.isOk, requirement.ok)) return false;
    if (!flagQuery(this.isImplementation, requirement.citation)) return false;
    if (!flagQuery(this.isImplication, requirement.implication)) return false;
    if (!flagQuery(this.isException, requirement.exception)) return false;
    if (!flagQuery(this.isTest, requirement.test)) return false;
    if (!flagQuery(this.isTodo, requirement.todo)) return false;

    if (this.level.size) {
      if (!this.level.has(requirement.level)) return false;
    }

    if (this.specifications.size) {
      if (!this.specifications.has(requirement.specification.id)) return false;
    }

    return true;
  }
}

function flagQuery(flag: Flag, value: boolean): boolean {
  if (flag === null) return true;
  return flag === value;
}
