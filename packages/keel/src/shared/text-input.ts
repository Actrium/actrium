import { useState } from 'react';

/** A text edit and its revision for the Guest-to-Host round trip. */
export interface TextInputEdit {
  text: string;
  eventCount: number;
}

/** Private synchronization between the Guest runtime and Host presets. */
export interface TextInputSyncProps {
  __keelTextInput?: {
    eventCount: number;
    onEdit: (edit: TextInputEdit) => void;
  };
}

export function getTextInputSync(sync: TextInputSyncProps['__keelTextInput']) {
  if (
    typeof sync?.onEdit === 'function' &&
    Number.isSafeInteger(sync.eventCount) &&
    sync.eventCount >= 0
  ) {
    return sync;
  }
  return undefined;
}

/** Accept writes at the latest edit and retain the last accepted display value. */
export function useTextInputValue(
  value: string | undefined,
  eventCount: number | undefined,
  defaultValue: string | undefined
) {
  const [display, recordEdit] = useState<{ text: string | undefined; eventCount: number }>({
    text: value ?? defaultValue,
    eventCount: 0,
  });
  const current = eventCount === display.eventCount;
  if (current && display.text !== value) {
    recordEdit({ text: value, eventCount: display.eventCount });
  }
  return { value: eventCount === undefined || current ? value : display.text, recordEdit };
}
