// Tab
export { TabEvents } from './tab/TabEvents';
export type { TabFullscreenEvent } from './tab/TabEvents';
export { openTab, exitFullscreen } from './tab/TabModule';

// Browser Data
export {
  clearBrowserBookmarks,
  clearBrowserHistory,
  deleteBrowserBookmark,
  deleteBrowserHistoryItem,
  getBrowserSyncStatus,
  hasBrowserDataModule,
  hasBrowserSyncModule,
  loadBrowserBookmarks,
  loadRecentBrowserHistory,
  syncBrowserDataNow,
} from './browser-data/BrowserDataModule';
export type {
  BrowserSyncStatus,
  BrowserSyncStatusName,
} from './browser-data/BrowserDataModule';

// Login
export { LoginBridge, LOGIN_RESULT_KEYS, LOGIN_TYPES } from './login/LoginBridge';
export type { LoginType, LoginResultEvent } from './login/LoginBridge';

// Extension
export { ExtensionEvents } from './extension/ExtensionEvents';
export type { ExtensionPanelVisibleChangeEvent } from './extension/ExtensionEvents';

// Container
export { ContainerEvents } from './container/ContainerEvents';
export type { ContainerSizeChangeEvent } from './container/ContainerEvents';

// Global Search
export { GlobalSearchEvents } from './globalsearch/GlobalSearchEvents';
export type { SearchInputChangedEvent } from './globalsearch/GlobalSearchEvents';

// Email
export { EmailBridge } from './email/EmailBridge';

// Speech
export { setLanguage, feedAudio, stop } from './speech/SpeechBridge';

// TTS
export {
  speak,
  pause,
  resume,
  stop as stopTTS,
  getAvailableLanguages,
} from './tts/TTSBridge';
export type { TTSSpeakOptions } from './tts/TTSBridge';
export type { TTSEventPayload, TTSFinishEvent } from './tts/TTSEvents';
export { TTSEvents } from './tts/TTSEvents';
