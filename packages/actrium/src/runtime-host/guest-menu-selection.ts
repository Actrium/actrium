export function completeGuestMenu(
  selection: Promise<string | null>,
  release: () => void,
  select: (value: string) => void | PromiseLike<void>,
  onError: (error: unknown) => void,
): Promise<void> {
  return selection
    .finally(release)
    .then((value) => { if (typeof value === 'string') return select(value); })
    .catch(onError);
}
