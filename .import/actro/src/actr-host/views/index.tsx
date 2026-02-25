import { useTabIdMapStore } from '../store/useTabIdMapStore';
import { TabConfig } from '../types';
import Chat from './Chat';
import LoadApp from './LoadApp';

import { useTab } from '../contexts/TabContext';

const Main = () => {
  const { tab } = useTab();
  const appId = useTabIdMapStore((state) => state.tabIdMap[tab.id]);

  if (!appId) {
    return <LoadApp />;
  }

  return <Chat />;
};

export default Main;
