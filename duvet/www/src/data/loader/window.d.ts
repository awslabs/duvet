import { IReport } from '../raw-report';

declare const reports: () => Promise<IReport[]>;

export default reports;
