import { createContext, useContext } from 'react';
import { Report as IReport } from '../data/report';

export const Report = createContext(new IReport({}));

export const useReport = (): IReport => useContext(Report);
