import { describe, expect, test } from 'bun:test';
import { historySections, historySiteLabel } from './history-sections';
import type { HistoryItemData } from './types';

function visit(id: string, day: number, hour: number): HistoryItemData {
  return { id, title: id, visitTime: new Date(2026, 10, day, hour).getTime() };
}

describe('historySections', () => {
  test('groups visits by local calendar day and preserves native order', () => {
    const sameDay = [visit('latest', 17, 23), visit('earlier', 17, 1)];
    const previousDay = [visit('previous', 16, 23)];

    const sections = historySections([...sameDay, ...previousDay]);

    expect(sections.map(section => section.data)).toEqual([sameDay, previousDay]);
    expect(sections[0].key).not.toEqual(sections[1].key);
  });

  test('responds to an empty native snapshot', () => {
    expect(historySections([])).toEqual([]);
  });

  test('includes relative labels for today and yesterday only', () => {
    const now = new Date(2026, 10, 17, 12);
    const format = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });
    const sections = historySections([visit('today', 17, 1), visit('yesterday', 16, 23), visit('older', 15, 23)], now);

    expect(sections[0].title.toLocaleLowerCase()).toStartWith(format.format(0, 'day').toLocaleLowerCase());
    expect(sections[1].title.toLocaleLowerCase()).toStartWith(format.format(-1, 'day').toLocaleLowerCase());
    expect(sections[2].title).toBe(new Date(2026, 10, 15).toLocaleDateString(undefined, { dateStyle: 'full' }));
  });
});

describe('historySiteLabel', () => {
  test('shows a hostname without displaying URL credentials, port or path', () => {
    expect(historySiteLabel('https://person@example.com:8080/path?q=1')).toBe('example.com');
    expect(historySiteLabel('http://[::1]:8080/path')).toBe('[::1]');
  });

  test('leaves the site label empty for non-web addresses', () => {
    expect(historySiteLabel('actr://example.com/instance')).toBeUndefined();
    expect(historySiteLabel()).toBeUndefined();
  });
});
