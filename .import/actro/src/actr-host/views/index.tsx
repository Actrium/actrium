import { useTabIdMapStore } from '../store/useTabIdMapStore';
import { TabConfig } from '../types';
import Chat from './Chat';
import LoadApp from './LoadApp';

import { useTab } from '../contexts/TabContext';

const ENABLE_TEMP_CHAT_MOCK_ENTRY = false;

const Main = () => {
  const { tab } = useTab();
  const appId = useTabIdMapStore((state) => state.tabIdMap[tab.id]);

  if (ENABLE_TEMP_CHAT_MOCK_ENTRY) {
    return <Chat showTemporaryMock />;
  }

  if (!appId) {
    return <LoadApp />;
  }

  return <Chat />;
};

export default Main;
