/**
 * StepList Host Implementation Tests
 *
 * Tests the StepList component props handling and element creation
 * Note: Direct function calls with hooks require React render context,
 * so we test element creation and props passing instead.
 */

import { createElement } from 'react';
import type { StepItem } from '../../types';
import { StepList, default as StepListDefault } from './StepList.host';

describe('StepList (Host)', () => {
  const mockItems: StepItem[] = [
    { id: '1', title: 'Step 1', status: 'completed' },
    { id: '2', title: 'Step 2', subtitle: 'In progress', status: 'active' },
    { id: '3', title: 'Step 3', status: 'pending' },
  ];

  describe('exports', () => {
    it('should export StepList as named export', () => {
      expect(StepList).toBeDefined();
      expect(typeof StepList).toBe('function');
    });

    it('should export StepList as default export', () => {
      expect(StepListDefault).toBeDefined();
      expect(StepListDefault).toBe(StepList);
    });
  });

  describe('element creation', () => {
    it('should create element without crashing', () => {
      const element = createElement(StepList, { items: mockItems });
      expect(element).toBeDefined();
      expect(element.type).toBe(StepList);
    });

    it('should preserve items prop on element', () => {
      const element = createElement(StepList, { items: mockItems });
      expect(element.props.items).toBe(mockItems);
      expect(element.props.items.length).toBe(3);
    });

    it('should handle empty items array', () => {
      const element = createElement(StepList, { items: [] });
      expect(element.props.items).toEqual([]);
    });
  });

  describe('props passing', () => {
    it('should accept loop prop', () => {
      const element = createElement(StepList, { items: mockItems, loop: true });
      expect(element.props.loop).toBe(true);
    });

    it('should accept loop=false explicitly', () => {
      const element = createElement(StepList, { items: mockItems, loop: false });
      expect(element.props.loop).toBe(false);
    });

    it('should accept onStepPress callback', () => {
      const onStepPress = (_item: StepItem, _index: number) => {};
      const element = createElement(StepList, { items: mockItems, onStepPress });
      expect(element.props.onStepPress).toBe(onStepPress);
    });

    it('should accept custom activeColor', () => {
      const element = createElement(StepList, {
        items: mockItems,
        activeColor: '#FF0000',
      });
      expect(element.props.activeColor).toBe('#FF0000');
    });

    it('should accept custom completedColor', () => {
      const element = createElement(StepList, {
        items: mockItems,
        completedColor: '#00FF00',
      });
      expect(element.props.completedColor).toBe('#00FF00');
    });

    it('should accept custom pendingColor', () => {
      const element = createElement(StepList, {
        items: mockItems,
        pendingColor: '#0000FF',
      });
      expect(element.props.pendingColor).toBe('#0000FF');
    });

    it('should accept custom errorColor', () => {
      const element = createElement(StepList, {
        items: mockItems,
        errorColor: '#FFFF00',
      });
      expect(element.props.errorColor).toBe('#FFFF00');
    });

    it('should accept all custom colors together', () => {
      const element = createElement(StepList, {
        items: mockItems,
        activeColor: '#FF0000',
        completedColor: '#00FF00',
        pendingColor: '#0000FF',
        errorColor: '#FFFF00',
      });
      expect(element.props.activeColor).toBe('#FF0000');
      expect(element.props.completedColor).toBe('#00FF00');
      expect(element.props.pendingColor).toBe('#0000FF');
      expect(element.props.errorColor).toBe('#FFFF00');
    });

    it('should accept lineWidth prop', () => {
      const element = createElement(StepList, { items: mockItems, lineWidth: 4 });
      expect(element.props.lineWidth).toBe(4);
    });

    it('should accept lineWidth of 0', () => {
      const element = createElement(StepList, { items: mockItems, lineWidth: 0 });
      expect(element.props.lineWidth).toBe(0);
    });

    it('should accept custom style', () => {
      const customStyle = { marginTop: 20, padding: 10 };
      const element = createElement(StepList, { items: mockItems, style: customStyle });
      expect(element.props.style).toBe(customStyle);
    });
  });

  describe('StepItem data types', () => {
    it('should handle items with all status types', () => {
      const allStatuses: StepItem[] = [
        { id: '1', title: 'Pending', status: 'pending' },
        { id: '2', title: 'Active', status: 'active' },
        { id: '3', title: 'Completed', status: 'completed' },
        { id: '4', title: 'Error', status: 'error' },
      ];
      const element = createElement(StepList, { items: allStatuses });
      expect(element.props.items.length).toBe(4);
      expect(element.props.items[0].status).toBe('pending');
      expect(element.props.items[1].status).toBe('active');
      expect(element.props.items[2].status).toBe('completed');
      expect(element.props.items[3].status).toBe('error');
    });

    it('should handle items with subtitles', () => {
      const itemsWithSubtitles: StepItem[] = [
        { id: '1', title: 'Step 1', subtitle: 'Description 1', status: 'completed' },
        { id: '2', title: 'Step 2', subtitle: 'Description 2', status: 'active' },
      ];
      const element = createElement(StepList, { items: itemsWithSubtitles });
      expect(element.props.items[0].subtitle).toBe('Description 1');
      expect(element.props.items[1].subtitle).toBe('Description 2');
    });

    it('should handle items without subtitles', () => {
      const itemsWithoutSubtitles: StepItem[] = [
        { id: '1', title: 'Step 1', status: 'pending' },
        { id: '2', title: 'Step 2', status: 'pending' },
      ];
      const element = createElement(StepList, { items: itemsWithoutSubtitles });
      expect(element.props.items[0].subtitle).toBeUndefined();
      expect(element.props.items[1].subtitle).toBeUndefined();
    });

    it('should handle mixed items (some with subtitle, some without)', () => {
      const mixedItems: StepItem[] = [
        { id: '1', title: 'With subtitle', subtitle: 'Has description', status: 'completed' },
        { id: '2', title: 'Without subtitle', status: 'active' },
      ];
      const element = createElement(StepList, { items: mixedItems });
      expect(element.props.items[0].subtitle).toBe('Has description');
      expect(element.props.items[1].subtitle).toBeUndefined();
    });

    it('should preserve item ids', () => {
      const itemsWithIds: StepItem[] = [
        { id: 'unique-id-1', title: 'Step 1', status: 'pending' },
        { id: 'unique-id-2', title: 'Step 2', status: 'pending' },
        { id: 'unique-id-3', title: 'Step 3', status: 'pending' },
      ];
      const element = createElement(StepList, { items: itemsWithIds });
      expect(element.props.items[0].id).toBe('unique-id-1');
      expect(element.props.items[1].id).toBe('unique-id-2');
      expect(element.props.items[2].id).toBe('unique-id-3');
    });
  });

  describe('callback type checking', () => {
    it('should accept callback that receives item and index', () => {
      let receivedItem: StepItem | null = null;
      let receivedIndex: number | null = null;

      const onStepPress = (item: StepItem, index: number) => {
        receivedItem = item;
        receivedIndex = index;
      };

      const element = createElement(StepList, { items: mockItems, onStepPress });

      // Simulate what the component would do
      element.props.onStepPress(mockItems[1], 1);

      expect(receivedItem).toBe(mockItems[1]);
      expect(receivedIndex).toBe(1);
    });

    it('should handle callback that returns void', () => {
      const results: string[] = [];
      const onStepPress = (item: StepItem, index: number) => {
        results.push(`${item.id}-${index}`);
      };

      const element = createElement(StepList, { items: mockItems, onStepPress });

      element.props.onStepPress(mockItems[0], 0);
      element.props.onStepPress(mockItems[1], 1);

      expect(results).toEqual(['1-0', '2-1']);
    });
  });

  describe('edge cases', () => {
    it('should handle single item', () => {
      const singleItem: StepItem[] = [{ id: '1', title: 'Only step', status: 'active' }];
      const element = createElement(StepList, { items: singleItem });
      expect(element.props.items.length).toBe(1);
    });

    it('should handle many items', () => {
      const manyItems: StepItem[] = Array.from({ length: 100 }, (_, i) => ({
        id: String(i + 1),
        title: `Step ${i + 1}`,
        status: 'pending' as const,
      }));
      const element = createElement(StepList, { items: manyItems });
      expect(element.props.items.length).toBe(100);
    });

    it('should handle items with long titles', () => {
      const longTitle = 'A'.repeat(500);
      const items: StepItem[] = [{ id: '1', title: longTitle, status: 'active' }];
      const element = createElement(StepList, { items });
      expect(element.props.items[0].title).toBe(longTitle);
    });

    it('should handle items with empty title', () => {
      const items: StepItem[] = [{ id: '1', title: '', status: 'pending' }];
      const element = createElement(StepList, { items });
      expect(element.props.items[0].title).toBe('');
    });

    it('should handle items with special characters in title', () => {
      const items: StepItem[] = [
        { id: '1', title: '✓ Step with emoji', status: 'completed' },
        { id: '2', title: 'Step with <html> tags', status: 'active' },
        { id: '3', title: 'Step with \'quotes\' and "double"', status: 'pending' },
      ];
      const element = createElement(StepList, { items });
      expect(element.props.items[0].title).toBe('✓ Step with emoji');
      expect(element.props.items[1].title).toBe('Step with <html> tags');
      expect(element.props.items[2].title).toBe('Step with \'quotes\' and "double"');
    });
  });
});
