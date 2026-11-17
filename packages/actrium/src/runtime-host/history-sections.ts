import type { HistoryItemData } from './types';

export function historySections(items: readonly HistoryItemData[], now = new Date()) {
  const sections = new Map<string, { key: string; title: string; data: HistoryItemData[] }>();
  const today = new Date(now);
  today.setHours(0, 0, 0, 0);
  const yesterday = new Date(today);
  yesterday.setDate(yesterday.getDate() - 1);
  const relative = typeof Intl !== 'undefined' && typeof Intl.RelativeTimeFormat === 'function'
    ? new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })
    : undefined;
  for (const item of items) {
    const date = new Date(item.visitTime);
    const day = date.setHours(0, 0, 0, 0);
    const key = String(day);
    const existing = sections.get(key);
    if (existing) {
      existing.data.push(item);
    } else {
      const fullDate = date.toLocaleDateString(undefined, { dateStyle: 'full' });
      const label = day === today.getTime() ? relative?.format(0, 'day') ?? 'Today'
        : day === yesterday.getTime() ? relative?.format(-1, 'day') ?? 'Yesterday'
        : '';
      const title = label ? `${label[0].toLocaleUpperCase()}${label.slice(1)} – ${fullDate}` : fullDate;
      sections.set(key, { key, title, data: [item] });
    }
  }
  return Array.from(sections.values());
}

export function historySiteLabel(address?: string): string | undefined {
  if (!address) return undefined;
  if (/^actro?:\/\//i.test(address)) return 'ACTR';
  return address.match(/^https?:\/\/(?:[^/?#]*@)?(\[[^\]]+\]|[^:/?#]+)/i)?.[1];
}
