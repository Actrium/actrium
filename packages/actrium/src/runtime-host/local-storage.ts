import type { LocalStorageKeyInput, LocalStorageSetInput } from './contract';

type LocalStorageOperation = 'getItem' | 'setItem' | 'removeItem';

export type LocalStorageCall = (
  operation: LocalStorageOperation,
  input: LocalStorageKeyInput | LocalStorageSetInput
) => Promise<string | null>;

type NativeLocalStorage = {
  perform(
    tabId: string,
    documentId: string,
    operation: LocalStorageOperation,
    key: string,
    value: string | null
  ): Promise<string | null>;
};

export function bindNativeLocalStorage(
  bridge: NativeLocalStorage | undefined,
  tabId: string,
  documentId: string | undefined
): LocalStorageCall | undefined {
  if (!bridge?.perform || !documentId) return undefined;
  return (operation, input) => bridge.perform(
    tabId, documentId, operation, input.key, 'value' in input ? input.value : null
  );
}
