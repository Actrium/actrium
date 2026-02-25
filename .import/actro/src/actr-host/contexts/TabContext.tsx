import React, { createContext, useContext, ReactNode } from 'react';
import { TabConfig } from '../types';

interface TabContextType {
  tab: TabConfig;
}

const TabContext = createContext<TabContextType | undefined>(undefined);

export const TabProvider: React.FC<{ tab: TabConfig; children: ReactNode }> = ({ tab, children }) => {
  return (
    <TabContext.Provider value={{ tab }}>
      {children}
    </TabContext.Provider>
  );
};

export const useTab = () => {
  const context = useContext(TabContext);
  if (context === undefined) {
    throw new Error('useTab must be used within a TabProvider');
  }
  return context;
};
