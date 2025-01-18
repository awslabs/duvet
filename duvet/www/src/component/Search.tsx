import { default as React, useState, useEffect } from 'react';
import {
  Report,
  Specification,
  Section,
  Annotation,
  Status,
  RequirementStats,
} from '../data/report';
import { Query, Mode, RangeInclusive } from '../data/query';
import { Link } from './Link';
import { Select, MultiSelect, Flag as FlagInput } from './Input';
import { Button } from './Button';
import clsx from 'clsx';
import { useSearchParams } from 'react-router';

export interface IProps {
  report: Report;
}

export function Search({ report }: IProps) {
  const [query, setQuery] = useState(() => new Query());

  let results = null;
  if (query.mode == Mode.SPEC)
    results = <SearchSpecifications report={report} query={query} />;
  if (query.mode == Mode.SECTION)
    results = <SearchSections report={report} query={query} />;
  if (query.mode == Mode.REQUIREMENT)
    results = <SearchRequirements report={report} query={query} />;

  return (
    <div className="flex flex-row">
      <div className="block min-w-80 max-w-80 mr-4">
        <div className="rounded border-slate-400 dark:border-neutral-600 border-2 pb-2">
          <SearchForm report={report} query={query} setQuery={setQuery} />
        </div>
      </div>
      <div className="block flex-grow">{results}</div>
    </div>
  );
}

const Field: React.FC<{ label: string; description?: string }> = ({
  label,
  description,
  children,
}) => (
  <div className="w-full x-3 mb-6 md:mb-0 mt-3 px-3">
    <label>
      <span className="block uppercase tracking-wide text-gray-700 dark:text-neutral-200 text-xs font-bold mb-2">
        {label}
      </span>
      <span>
        {description ? (
          <span className="block text-gray-700 dark:text-neutral-200 text-sm mb-2">
            {description}
          </span>
        ) : null}
      </span>
      <div className="relative mt-1">{children}</div>
    </label>
  </div>
);

const Card: React.FC<{ title?: string; url?: string }> = ({
  title,
  url,
  children,
}) => (
  <div className="leading-5 mb-6 pb-6 border-b-slate-400 dark:border-b-neutral-600 border-b-2">
    <span className="block text-2xl mb-2 cursor-pointer hover:underline font-medium text-slate-800 dark:text-blue-300">
      <Link to={url}>{title}</Link>
    </span>
    {children}
  </div>
);

function CardStatus({ label, children }) {
  return (
    <li className="inline-block mr-4 text-left text-sm">
      <span className="text-neutral-700 dark:text-neutral-400 font-light">
        {label ? `${label}: ` : ''}
      </span>
      {children}
    </li>
  );
}

function CardStatusGroup({ children }) {
  return <ul className="list-none mt-2">{children}</ul>;
}

export function SearchForm({
  report,
  query,
  setQuery,
}: {
  report: Report;
  query: Query;
  setQuery: (query: Query) => void;
}) {
  const fields =
    query.mode === Mode.REQUIREMENT ? (
      <>
        {report.specifications.length > 1
          ? ((specs) => (
              <Field label="Specification">
                <MultiSelect
                  size={specs.length + 1}
                  value={query.specifications}
                  onChange={(value) =>
                    setQuery(query.withSpecifications(value))
                  }
                >
                  {specs.map((spec) => (
                    <option value={spec.id} key={spec.id}>
                      {spec.title}
                    </option>
                  ))}
                </MultiSelect>
              </Field>
            ))(
              report.specifications.byIdx.filter(
                (spec) => spec.requirements.length > 0,
              ),
            )
          : null}
        <Field label="Ok" description="Is Complete or has an Exception">
          <FlagInput
            value={query.isOk}
            onChange={(value) => setQuery(query.withIsOk(value))}
          />
        </Field>
        <Field
          label="Complete"
          description="Is Implementation+Test or Implication"
        >
          <FlagInput
            value={query.isComplete}
            onChange={(value) => setQuery(query.withIsComplete(value))}
          />
        </Field>
        <Field label="Implementation">
          <FlagInput
            value={query.isImplementation}
            onChange={(value) => setQuery(query.withIsImplementation(value))}
          />
        </Field>
        <Field label="Implication">
          <FlagInput
            value={query.isImplication}
            onChange={(value) => setQuery(query.withIsImplication(value))}
          />
        </Field>
        <Field label="Exception">
          <FlagInput
            value={query.isException}
            onChange={(value) => setQuery(query.withIsException(value))}
          />
        </Field>
        <Field label="Test">
          <FlagInput
            value={query.isTest}
            onChange={(value) => setQuery(query.withIsTest(value))}
          />
        </Field>
        <Field label="TODO">
          <FlagInput
            value={query.isTodo}
            onChange={(value) => setQuery(query.withIsTodo(value))}
          />
        </Field>
      </>
    ) : (
      <>
        <Field label="Has Requirements">
          <FlagInput
            value={query.hasRequirements}
            onChange={(value) => setQuery(query.withHasRequirements(value))}
          />
        </Field>
        <Field label="Complete Percent">
          <DoubleRangeInput
            range={new RangeInclusive([0, 100])}
            value={query.completeRange}
            onChange={(value) => setQuery(query.withCompleteRange(value))}
          />
        </Field>
      </>
    );

  return (
    <>
      <Field label="Search for">
        <Select
          value={query.mode}
          onChange={(evt) => {
            setQuery(query.withMode(evt.target.value));
          }}
        >
          <option value={Mode.SPEC}>{Mode.SPEC}</option>
          <option value={Mode.SECTION}>{Mode.SECTION}</option>
          <option value={Mode.REQUIREMENT}>{Mode.REQUIREMENT}</option>
        </Select>
      </Field>
      <Field label="Level">
        <MultiSelect
          size="4"
          value={query.level}
          onChange={(value) => setQuery(query.withLevel(value))}
        >
          <option value="MUST">MUST</option>
          <option value="SHOULD">SHOULD</option>
          <option value="MAY">MAY</option>
        </MultiSelect>
      </Field>
      {fields}
    </>
  );
}

export function SearchSpecifications({
  report,
  query,
}: {
  report: Report;
  query: Query;
}) {
  const specs = query.searchSpecs(report, (item, idx) => (
    <SearchSpecCard specification={item} key={idx} />
  ));
  return (
    <SearchResults
      mode={query.mode}
      total={report.specifications.length}
      items={specs}
    />
  );
}

export function SearchSpecCard({
  specification,
}: {
  specification: Specification;
  key: number;
}) {
  const percent = specification.stats.overall.percent('complete');
  return (
    <Card url={specification.url} title={specification.title}>
      <CardStatusGroup>
        <SearchCardStats stats={specification.stats} />
        <CardStatus label="References">
          {specification.references.length}
        </CardStatus>
      </CardStatusGroup>
    </Card>
  );
}

export function SearchSections({
  report,
  query,
}: {
  report: Report;
  query: Query;
}) {
  const sections = query.searchSections(report, (item, idx) => (
    <SearchSectionCard section={item} key={idx} />
  ));
  return (
    <SearchResults
      mode={query.mode}
      total={report.sections.length}
      items={sections}
    />
  );
}

export function SearchSectionCard({
  section,
}: {
  section: Section;
  key: number;
}) {
  function formatStat(stat) {
    const percent = stat.percent('complete');
    return `${percent.value} / ${percent.total} (${percent})`;
  }
  return (
    <Card url={section.url} title={section.title}>
      <CardStatusGroup>
        <SearchCardStats stats={section.stats} />
        <CardStatus label="References">{section.references.length}</CardStatus>
      </CardStatusGroup>
    </Card>
  );
}

function SearchCardStats({ stats }: { stats: RequirementStats }) {
  function formatStat(stat) {
    const percent = stat.percent('complete');
    return `${percent.value} / ${percent.total} (${percent})`;
  }

  return (
    <>
      {stats.overall.total ? (
        <CardStatus label="Requirements">
          {formatStat(stats.overall)}
        </CardStatus>
      ) : null}
      {stats.MUST.total ? (
        <CardStatus label="MUST">{formatStat(stats.MUST)}</CardStatus>
      ) : null}
      {stats.SHOULD.total ? (
        <CardStatus label="SHOULD">{formatStat(stats.SHOULD)}</CardStatus>
      ) : null}
      {stats.MAY.total ? (
        <CardStatus label="MAY">{formatStat(stats.MAY)}</CardStatus>
      ) : null}
    </>
  );
}

export function SearchRequirements({
  report,
  query,
}: {
  report: Report;
  query: Query;
}) {
  const sections = query.searchRequirements(report, (item, idx) => (
    <SearchRequirementCard requirement={item} key={idx} />
  ));
  return (
    <SearchResults
      mode={query.mode}
      total={report.requirements.length}
      items={sections}
    />
  );
}

export function SearchRequirementCard({
  requirement,
}: {
  requirement: Annotation;
  key: number;
}) {
  return (
    <Card
      url={requirement.url}
      title={`${requirement.specification.title}#${requirement.section.shortId}`}
    >
      <p className="border-l border-l-neutral-400 pl-5">{requirement.text}</p>
      {requirement.canComment ? (
        <p className="my-2">{requirement.allComments}</p>
      ) : null}
      <RequirementStatus requirement={requirement} />
    </Card>
  );
}

function RequirementStatus({ requirement }: { requirement: Annotation }) {
  const statuses = [];
  if (requirement.complete) statuses.push('Complete');
  if (requirement.citation) statuses.push('Implementation');
  if (requirement.implication) statuses.push('Implication');
  if (requirement.exception) statuses.push('Exception');
  if (requirement.test) statuses.push('Test');
  if (requirement.todo) statuses.push('Todo');

  if (requirement.citation && !requirement.test) statuses.push('Missing Test');
  if (requirement.test && !requirement.citation)
    statuses.push('Missing Implementation');

  if (!statuses.length) statuses.push('Unknown');

  return (
    <CardStatusGroup>
      {requirement.level ? (
        <CardStatus label="Level">{requirement.level}</CardStatus>
      ) : null}
      <CardStatus label="Status">
        {statuses.map((status, idx) => (
          <span
            key={idx}
            className={clsx(
              'ml-1 font-mono uppercase text-xs rounded text-neutral-300 py-1 px-2',
              {
                'bg-green-600': status === 'Complete',
                'bg-blue-600': status === 'Implementation',
                'bg-yellow-600': status === 'Implication',
                'bg-gray-600': status === 'Exception',
                'bg-purple-600': status === 'Test',
                'bg-orange-600':
                  status === 'Todo' ||
                  status === 'Missing Test' ||
                  status === 'Missing Implementation',
                'bg-red-600': status === 'Unknown',
              },
            )}
          >
            {status}
          </span>
        ))}
      </CardStatus>
    </CardStatusGroup>
  );
}

function SearchResults<T>({
  mode,
  items,
  total,
}: {
  mode: Mode;
  items: React.ReactNode[];
  total: number;
}) {
  const [searchParams, setSearchParams] = useSearchParams();
  let page = Math.max(parseInt(searchParams.get('page') || '1') - 1, 0);
  const itemsPerPage = parseInt(searchParams.get('items') || '20');

  useEffect(() => {
    window.scrollTo(0, 0);
  }, [page]);

  function setPage(page: number) {
    setSearchParams((params) => {
      if (page === 0) params.delete('page');
      else params.set('page', (page + 1).toString());
      return params;
    });
  }

  const pages = Math.max(Math.ceil(items.length / itemsPerPage), 1);
  page = Math.min(page, pages - 1);

  const pagination =
    pages === 1 ? null : (
      <div className="w-full flex justify-center sticky bottom-7">
        <div className="inline-flex bg-white dark:bg-black rounded drop-shadow">
          <Button
            disabled={page === 0}
            className="rounded-l"
            onClick={() => setPage(0)}
          >
            First
          </Button>
          <Button
            disabled={page === 0}
            onClick={() => setPage(Math.max(0, page - 1))}
          >
            Previous
          </Button>
          <Button
            disabled={page === pages - 1}
            onClick={() => setPage(Math.min(pages - 1, page + 1))}
          >
            Next
          </Button>
          <Button
            disabled={page === pages - 1}
            className="rounded-r"
            onClick={() => setPage(pages - 1)}
          >
            Last
          </Button>
        </div>
      </div>
    );

  let start = page * itemsPerPage;
  let end = Math.min(start + itemsPerPage, items.length);

  const info = items.length ? (
    <div className="block text-2xl leading-loose">
      Showing results {start + 1}-{end} of {items.length}{' '}
      {mode.toLocaleLowerCase()}s.
    </div>
  ) : (
    <div className="block text-2xl leading-loose">
      No {mode.toLocaleLowerCase()}s found for the given query.
    </div>
  );

  return (
    <>
      {info}
      {...items.slice(start, end)}
      {pagination}
    </>
  );
}

function DoubleRangeInput({
  range: { min, max },
  value,
  onChange,
}: {
  range: RangeInclusive;
  value: RangeInclusive;
  onChange: (value: RangeInclusive) => void;
}) {
  type Value = string | number | null;

  function parseValue(value: Value, fallback: number): number {
    if (typeof value == 'number') return value;
    if (typeof value == 'string') return parseInt(value);
    return fallback;
  }

  function wrappedChange(minV: Value, maxV: Value) {
    const newValue = new RangeInclusive([
      parseValue(minV, min),
      parseValue(maxV, max),
    ]);
    onChange(newValue);
  }

  return (
    <>
      <input
        type="range"
        min={min}
        max={max}
        value={value.min}
        onChange={(evt) => wrappedChange(evt.target.value, value.max)}
      />
      <input
        type="range"
        min={min}
        max={max}
        value={value.max}
        onChange={(evt) => wrappedChange(value.min, evt.target.value)}
      />
      {value.toString()}
    </>
  );
}
