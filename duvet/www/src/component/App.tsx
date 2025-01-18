import { default as React, useState } from 'react';
import { Report } from './Report';
import { Routes } from './Routes';
import { Report as IReport } from '../data/report';
import { Body } from './Body';
import { Select } from './Input';

export function App({ reports }: { reports: IReport[] }) {
  if (reports.length === 0) {
    return (
      <Body>
        <div>No reports found</div>
      </Body>
    );
  }

  // TODO add nav
  const root = (
    <Body>
      <Routes />
    </Body>
  );

  if (reports.length == 1) {
    return <Report.Provider value={reports[0]}>{root}</Report.Provider>;
  }

  const [reportIdx, setReportIdx] = useState(0);
  const report = reports[reportIdx];

  return (
    <>
      <ReportPicker reports={reports} idx={reportIdx} onChange={setReportIdx} />
      <Report.Provider value={report}>{root}</Report.Provider>
    </>
  );
}

function ReportPicker({
  reports,
  idx,
  onChange,
}: {
  reports: IReport[];
  idx: number;
  onChange: (idx: number) => void;
}) {
  return (
    <div className="bg-slate-500 w-full min-w-full p-2">
      <Select value={idx} onChange={(e) => onChange(parseInt(e.target.value))}>
        {reports.map((report, idx) => (
          <option key={idx} value={idx}>
            {report.title}
          </option>
        ))}
      </Select>
    </div>
  );
}
