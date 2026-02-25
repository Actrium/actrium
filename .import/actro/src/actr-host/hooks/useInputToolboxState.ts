import { useState } from 'react';
import { useAppStore } from '../store/useAppStore';

export const useInputToolboxState = (appId: string) => {
  const [isWebSearchActive, setIsWebSearchActive] = useState(false);
  const [isLocationActive, setIsLocationActive] = useState(false);
  const [isSendOnEnter, setIsSendOnEnter] = useState(true);

  const app = useAppStore((state) => state.apps[appId]);

  const showSearchEntry = app?.client?.enable_search !== false;
  const showLocationEntry = app?.client?.share_location !== false;
  const showNewlineEntry = app?.client?.send_on_enter !== false;
  const showMoreOptions = showLocationEntry || showNewlineEntry;

  const toggleWebSearch = () => setIsWebSearchActive((prev) => !prev);
  const toggleLocation = () => setIsLocationActive((prev) => !prev);
  const toggleSendOnEnter = () => setIsSendOnEnter((prev) => !prev);

  const toolboxProps = {
    isWebSearchActive,
    setIsWebSearchActive,
    isLocationActive,
    setIsLocationActive,
    isSendOnEnter,
    setIsSendOnEnter,
    showSearchEntry,
    showLocationEntry,
    showNewlineEntry,
    showMoreOptions,
  };

  return {
    ...toolboxProps,
    toolboxProps,
    toggleWebSearch,
    toggleLocation,
    toggleSendOnEnter,
  };
};
