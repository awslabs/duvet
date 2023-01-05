
export class Stats {
  constructor() {
    this.total = 0;
    this.complete = 0;
    this.incomplete = 0;
    this.citations = 0;
    this.implications = 0;
    this.tests = 0;
    this.exceptions = 0;
    this.todos = 0;
  }

  onRequirement(requirement) {
    this.total += 1;

    if (requirement.incomplete) this.incomplete += 1;
    else if (requirement.isOk) this.complete += 1;

    if (requirement.citation) this.citations += 1;
    if (requirement.implication) this.implications += 1;
    if (requirement.test) this.tests += 1;
    if (requirement.exception) this.exceptions += 1;
    if (requirement.todo) this.todos += 1;
  }

  onStat(stat) {
    this.total += stat.total;
    this.complete += stat.complete;
    this.incomplete += stat.incomplete;
    this.citations += stat.citations;
    this.implications += stat.implications;
    this.tests += stat.tests;
    this.exceptions += stat.exceptions;
    this.todos += stat.todos;
  }

  percent(field) {
    const percent = this.total ? this[field] / this.total : 0;
    return Number(percent).toLocaleString(undefined, {
      style: "percent",
      minimumFractionDigits: 0,
      maximumFractionDigits: 2,
    });
  }
}