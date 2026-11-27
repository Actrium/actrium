import type { LocalStorageKeyInput, LocalStorageReplaceInput, LocalStorageSetInput } from './contract';

type LocalStorageOperation = 'getItem' | 'setItem' | 'removeItem' | 'replaceItems';

export type LocalStorageCall = (
  operation: LocalStorageOperation,
  input: LocalStorageKeyInput | LocalStorageSetInput | LocalStorageReplaceInput
) => Promise<string | null>;

type NativeLocalStorage = {
  perform(
    tabId: string,
    documentId: string,
    operation: LocalStorageOperation,
    inputJSON: string
  ): Promise<string>;
};

export function bindNativeLocalStorage(
  bridge: NativeLocalStorage | undefined,
  tabId: string,
  documentId: string | undefined
): LocalStorageCall | undefined {
  if (!bridge?.perform || !documentId) return undefined;
  return async (operation, input) => {
    // JSON escapes embedded NULs before they cross React Native's string bridge.
    const json = await bridge.perform(tabId, documentId, operation, JSON.stringify(input));
    const value: unknown = JSON.parse(json);
    if (value !== null && typeof value !== 'string') {
      throw new Error('Invalid local storage result.');
    }
    return value;
  };
}
