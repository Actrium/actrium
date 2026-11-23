/** The document currently providing the selection in one guest surface. */
export interface DocumentSelectionHandle {
  getSelectedText(): string | null;
  clearSelection(): void;
}

export function createDocumentSelection(onAvailabilityChange?: (available: boolean) => void) {
  let owner: DocumentSelectionHandle | null = null;
  function replace(next: DocumentSelectionHandle | null) {
    if (owner === next) return;
    const previous = owner;
    owner = next;
    if (!previous || !next) onAvailabilityChange?.(next !== null);
    return previous;
  }
  return {
    update(handle: DocumentSelectionHandle, selected: boolean) {
      if (selected) replace(handle)?.clearSelection();
      else if (owner === handle) replace(null);
    },
    clear() {
      replace(null)?.clearSelection();
    },
    copy(writeText: (text: string) => void) {
      const text = owner?.getSelectedText();
      if (text) writeText(text);
    },
  };
}
