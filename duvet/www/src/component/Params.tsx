import {
  Specification as ISpecification,
  Section as ISection,
} from '../data/report';
import { useReport } from './Report';
import { useParams, useSearchParams } from 'react-router';

export const useSpecification = (): ISpecification | null => {
  const report = useReport();
  const { specification } = useParams();
  if (!specification) return null;
  return report.specifications.byId.get(specification);
};

export const useSection = (): ISection | null => {
  const specification = useSpecification();
  const [params, _setParams] = useSearchParams();
  const section = params.get('section');
  if (!section) return null;
  if (!specification) return null;
  return specification.sections.byId.get(section);
};
