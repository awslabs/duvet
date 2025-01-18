export interface IReport {
  title?: string;
  annotations?: IAnnotation[];
  blob_link?: string;
  issue_link?: string;
  refs?: IRef[];
  specifications?: ISpecifications;
  statuses?: IStatuses;
}

export type IAnnotationType =
  | 'CITATION'
  | 'EXCEPTION'
  | 'IMPLICATION'
  | 'SPEC'
  | 'TEST'
  | 'TODO';

export interface IAnnotation {
  line?: number;
  level?: ILevel;
  source?: string;
  target_path?: string;
  target_section?: string;
  comment?: string;
  type?: IAnnotationType;
}

export type IStatusName =
  | 'citation'
  | 'exception'
  | 'implication'
  | 'spec'
  | 'test'
  | 'todo';

export interface IRef {
  citation?: boolean;
  exception?: boolean;
  implication?: boolean;
  level?: ILevel;
  spec?: boolean;
  test?: boolean;
  todo?: boolean;
}

export type ILevel = 'MUST' | 'SHOULD' | 'MAY';

export interface ISpecifications {
  [id: string]: ISpecification;
}

export interface ISpecification {
  title?: string;
  format?: string;
  requirements?: number[];
  sections?: ISection[];
}

export interface ISection {
  id?: string;
  title?: string;
  lines?: ILine[];
  requirements?: number[];
}

export type ILine = string | ILineList[];

export type ILineList = string | ILineRegion;

export type ILineRegion = [number[], number, string];

export interface IStatuses {
  [id: string]: IStatus;
}

export interface IStatus {
  incomplete?: number;
  related?: number[];
  spec?: number;
  implication?: number;
  todo?: number;
  citation?: number;
  exception?: number;
  test?: number;
}
