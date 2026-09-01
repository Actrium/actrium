import { useTabIdMapStore } from '../../store/useTabIdMapStore';
import Chat2 from '../chat/Chat2';
import LoadApp from '../load/LoadApp';

import { useTab } from '../../contexts/TabContext';

const Main = () => {
  const { tab } = useTab();
  const appId = useTabIdMapStore((state) => state.tabIdMap[tab.id]);

  // No, I don't. appId Time to go first. LoadApp(sweeping page) Upon completion of the sweep, jump back and take it with you. appId.
  if (!appId) {
    return <LoadApp />;
  }
  return <Chat2 appId={appId} />;
};

export default Main;
