import { default as React } from 'react';
import { Specification as ISpecification } from '../data/report';
import { Link } from './Link';
import { Section, Scroller } from './Section';

export interface IProps {
  specification: ISpecification;
}

export function Specification({ specification: spec }: IProps) {
  return (
    <>
      <div className="flex justify-center">
        <div className="m-6">
          <h2 className="text-center font-semibold mt-24 text-4xl">
            <Link to={spec.url}>{spec.title}</Link>
          </h2>
          {spec.sections.byIdx.map((section, idx) => (
            <Section section={section} key={idx} />
          ))}
        </div>
      </div>
      <Scroller />
    </>
  );
}
