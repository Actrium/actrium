/** A text edit and its revision for the Guest-to-Host round trip. */
export interface TextInputEdit {
  text: string;
  eventCount: number;
}

/** Ignore Guest values based on an edit older than the Host's current text. */
export function resolveTextInputValue(
  value: string | undefined,
  valueEventCount: number | undefined,
  edit: TextInputEdit
): string | undefined {
  return valueEventCount === undefined || valueEventCount === edit.eventCount ? value : edit.text;
}
