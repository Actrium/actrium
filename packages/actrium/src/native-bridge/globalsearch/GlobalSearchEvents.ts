import { NativeEventEmitter, NativeModules } from 'react-native';
import { RUNTIME_NATIVE_EVENTS } from '../../actr-host/coreContract';

const { TabEventEmitter } = NativeModules;

export type SearchInputChangedEvent = {
  inputText: string;
  timestamp?: number;
};

type Unsubscribe = () => void;

function getEventEmitter(): NativeEventEmitter {
  return new NativeEventEmitter(TabEventEmitter);
}

export const GlobalSearchEvents = {
  // Listen to host search input changes,GlobalSearch The page only consumes input content that is transferred by the host and no longer has a self-administered input frame status.
  onSearchInputChanged: (
    callback: (event: SearchInputChangedEvent) => void,
  ): Unsubscribe => {
    const eventEmitter = getEventEmitter();

    const subscription = eventEmitter!.addListener(
      RUNTIME_NATIVE_EVENTS.searchInputChanged,
      callback,
    );
    return () => subscription.remove();
  },
};
