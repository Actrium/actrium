import { useTabIdMapStore } from '../../store/useTabIdMapStore';
import Chat from '../chat/Chat';
import Chat2 from '../chat/Chat2';
import LoadApp from '../load/LoadApp';

import { useTab } from '../../contexts/TabContext';

const ENABLE_TEMP_CHAT_MOCK_ENTRY = false;

const Main = () => {
  const { tab } = useTab();
  const appId = useTabIdMapStore((state) => state.tabIdMap[tab.id]);

  // 中文注释：没有 appId 时先走 LoadApp（扫码页），扫码完成后自动跳回并带上 appId。
  if (!appId) {
    return <LoadApp />;
  }
  return <Chat2 appId={appId} />

  if (ENABLE_TEMP_CHAT_MOCK_ENTRY) {
    return <Chat showTemporaryMock />;
  }

  return <Chat />;
};

export default Main;
