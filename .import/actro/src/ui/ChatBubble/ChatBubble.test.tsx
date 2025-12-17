/**
 * ChatBubble Host Implementation Tests
 *
 * Tests the ChatBubble component props handling and element creation
 */

import { createElement } from 'react';
import type { ChatBubbleProps } from '../../types';
import { ChatBubble, default as ChatBubbleDefault } from './ChatBubble.host';

describe('ChatBubble (Host)', () => {
  describe('exports', () => {
    it('should export ChatBubble as named export', () => {
      expect(ChatBubble).toBeDefined();
      expect(typeof ChatBubble).toBe('function');
    });

    it('should export ChatBubble as default export', () => {
      expect(ChatBubbleDefault).toBeDefined();
      expect(ChatBubbleDefault).toBe(ChatBubble);
    });
  });

  describe('element creation', () => {
    it('should create element without crashing', () => {
      const element = createElement(ChatBubble, { content: 'Hello' });
      expect(element).toBeDefined();
      expect(element.type).toBe(ChatBubble);
    });

    it('should preserve content prop', () => {
      const element = createElement(ChatBubble, { content: 'Test message' });
      expect(element.props.content).toBe('Test message');
    });
  });

  describe('props passing', () => {
    it('should accept content prop', () => {
      const element = createElement(ChatBubble, { content: 'Hello world' });
      expect(element.props.content).toBe('Hello world');
    });

    it('should accept isOwn=true', () => {
      const element = createElement(ChatBubble, { content: 'My message', isOwn: true });
      expect(element.props.isOwn).toBe(true);
    });

    it('should accept isOwn=false', () => {
      const element = createElement(ChatBubble, { content: 'Their message', isOwn: false });
      expect(element.props.isOwn).toBe(false);
    });

    it('should accept showTail prop', () => {
      const element = createElement(ChatBubble, { content: 'Message', showTail: false });
      expect(element.props.showTail).toBe(false);
    });

    it('should accept timestamp as string', () => {
      const element = createElement(ChatBubble, {
        content: 'Message',
        timestamp: '2024-01-15T10:30:00Z',
      });
      expect(element.props.timestamp).toBe('2024-01-15T10:30:00Z');
    });

    it('should accept timestamp as number', () => {
      const timestamp = Date.now();
      const element = createElement(ChatBubble, { content: 'Message', timestamp });
      expect(element.props.timestamp).toBe(timestamp);
    });

    it('should accept timestamp as Date', () => {
      const timestamp = new Date();
      const element = createElement(ChatBubble, { content: 'Message', timestamp });
      expect(element.props.timestamp).toBe(timestamp);
    });

    it('should accept custom style', () => {
      const style = { marginTop: 10 };
      const element = createElement(ChatBubble, { content: 'Message', style });
      expect(element.props.style).toBe(style);
    });
  });

  describe('status prop', () => {
    const statuses: ChatBubbleProps['status'][] = ['sending', 'sent', 'delivered', 'read', 'error'];

    for (const status of statuses) {
      it(`should accept status="${status}"`, () => {
        const element = createElement(ChatBubble, { content: 'Message', isOwn: true, status });
        expect(element.props.status).toBe(status);
      });
    }

    it('should accept undefined status', () => {
      const element = createElement(ChatBubble, { content: 'Message', isOwn: true });
      expect(element.props.status).toBeUndefined();
    });
  });

  describe('callback props', () => {
    it('should accept onPress callback', () => {
      const onPress = () => {};
      const element = createElement(ChatBubble, { content: 'Message', onPress });
      expect(element.props.onPress).toBe(onPress);
    });

    it('should accept onLongPress callback', () => {
      const onLongPress = () => {};
      const element = createElement(ChatBubble, { content: 'Message', onLongPress });
      expect(element.props.onLongPress).toBe(onLongPress);
    });

    it('should accept both onPress and onLongPress', () => {
      const onPress = () => {};
      const onLongPress = () => {};
      const element = createElement(ChatBubble, { content: 'Message', onPress, onLongPress });
      expect(element.props.onPress).toBe(onPress);
      expect(element.props.onLongPress).toBe(onLongPress);
    });

    it('should invoke onPress when called', () => {
      let called = false;
      const onPress = () => {
        called = true;
      };
      const element = createElement(ChatBubble, { content: 'Message', onPress });
      element.props.onPress();
      expect(called).toBe(true);
    });

    it('should invoke onLongPress when called', () => {
      let called = false;
      const onLongPress = () => {
        called = true;
      };
      const element = createElement(ChatBubble, { content: 'Message', onLongPress });
      element.props.onLongPress();
      expect(called).toBe(true);
    });
  });

  describe('children prop', () => {
    it('should accept children via createElement', () => {
      const child = createElement('span', {}, 'Custom content');
      // Pass children as third argument (canonical way)
      const element = createElement(ChatBubble, {}, child);
      expect(element.props.children).toBe(child);
    });

    it('should accept multiple children', () => {
      const child1 = createElement('span', { key: '1' }, 'First');
      const child2 = createElement('span', { key: '2' }, 'Second');
      const element = createElement(ChatBubble, {}, child1, child2);
      expect(element.props.children).toHaveLength(2);
    });
  });

  describe('edge cases', () => {
    it('should handle empty content', () => {
      const element = createElement(ChatBubble, { content: '' });
      expect(element.props.content).toBe('');
    });

    it('should handle long content', () => {
      const longContent = 'A'.repeat(1000);
      const element = createElement(ChatBubble, { content: longContent });
      expect(element.props.content).toBe(longContent);
    });

    it('should handle content with special characters', () => {
      const content = 'Hello 👋 <script>alert("xss")</script> "quotes"';
      const element = createElement(ChatBubble, { content });
      expect(element.props.content).toBe(content);
    });

    it('should handle multiline content', () => {
      const content = 'Line 1\nLine 2\nLine 3';
      const element = createElement(ChatBubble, { content });
      expect(element.props.content).toBe(content);
    });

    it('should handle content with unicode', () => {
      const content = '你好世界 مرحبا こんにちは';
      const element = createElement(ChatBubble, { content });
      expect(element.props.content).toBe(content);
    });
  });

  describe('own vs other message styling hints', () => {
    it('should pass isOwn=true for own messages', () => {
      const element = createElement(ChatBubble, {
        content: 'My message',
        isOwn: true,
        status: 'sent',
      });
      expect(element.props.isOwn).toBe(true);
      expect(element.props.status).toBe('sent');
    });

    it('should pass isOwn=false for other messages', () => {
      const element = createElement(ChatBubble, {
        content: 'Their message',
        isOwn: false,
      });
      expect(element.props.isOwn).toBe(false);
      // Status is typically only shown for own messages
      expect(element.props.status).toBeUndefined();
    });
  });

  describe('combined props scenarios', () => {
    it('should handle typical own message props', () => {
      const element = createElement(ChatBubble, {
        content: 'Hello!',
        isOwn: true,
        showTail: true,
        timestamp: '2024-01-15T10:30:00Z',
        status: 'delivered',
      });
      expect(element.props.content).toBe('Hello!');
      expect(element.props.isOwn).toBe(true);
      expect(element.props.showTail).toBe(true);
      expect(element.props.timestamp).toBe('2024-01-15T10:30:00Z');
      expect(element.props.status).toBe('delivered');
    });

    it('should handle typical other message props', () => {
      const element = createElement(ChatBubble, {
        content: 'Hi there!',
        isOwn: false,
        showTail: true,
        timestamp: '2024-01-15T10:31:00Z',
      });
      expect(element.props.content).toBe('Hi there!');
      expect(element.props.isOwn).toBe(false);
      expect(element.props.showTail).toBe(true);
      expect(element.props.timestamp).toBe('2024-01-15T10:31:00Z');
    });

    it('should handle consecutive message (no tail)', () => {
      const element = createElement(ChatBubble, {
        content: 'Follow up message',
        isOwn: true,
        showTail: false,
        timestamp: '2024-01-15T10:32:00Z',
        status: 'sent',
      });
      expect(element.props.showTail).toBe(false);
    });
  });
});
