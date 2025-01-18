import { useEffect } from 'react';
import { useReport } from './Report';

export function PageTitle({ value = null }) {
  const report = useReport();

  useEffect(() => {
    document.title = value ? `${report.title} - ${value}` : report.title;
  }, [report.title, value]);
}
