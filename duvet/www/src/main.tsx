import { default as React } from 'react';
import ReactDOM from 'react-dom/client';
import { HashRouter } from 'react-router';
import { App } from './component/App';
import { ScrollToTop } from './component/ScrollToTop';
import createReport from './data/report';
import { IReport } from './data/raw-report';
import './main.css';

export default async function boot(loadReport: () => Promise<IReport[]>) {
  const json = await loadReport();
  const reports = json.map(createReport);

  ReactDOM.createRoot(document.getElementById('root')).render(
    <React.StrictMode>
      <HashRouter>
        <ScrollToTop />
        <App reports={reports} />
      </HashRouter>
    </React.StrictMode>,
  );
}
