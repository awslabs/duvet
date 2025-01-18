import { default as React } from 'react';
import { useReport } from './Report';
import { Search } from './Search';

export function Index() {
  const report = useReport();
  return <Search report={report} />;
}
