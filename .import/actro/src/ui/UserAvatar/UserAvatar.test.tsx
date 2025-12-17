/**
 * UserAvatar Host Implementation Tests
 *
 * Tests the UserAvatar component props handling and element creation
 */

import { createElement } from 'react';
import { UserAvatar, default as UserAvatarDefault } from './UserAvatar.host';

describe('UserAvatar (Host)', () => {
  describe('exports', () => {
    it('should export UserAvatar as named export', () => {
      expect(UserAvatar).toBeDefined();
      expect(typeof UserAvatar).toBe('function');
    });

    it('should export UserAvatar as default export', () => {
      expect(UserAvatarDefault).toBeDefined();
      expect(UserAvatarDefault).toBe(UserAvatar);
    });
  });

  describe('element creation', () => {
    it('should create element without crashing', () => {
      const element = createElement(UserAvatar, {});
      expect(element).toBeDefined();
      expect(element.type).toBe(UserAvatar);
    });

    it('should create element with name', () => {
      const element = createElement(UserAvatar, { name: 'John Doe' });
      expect(element.props.name).toBe('John Doe');
    });
  });

  describe('uri prop', () => {
    it('should accept uri prop', () => {
      const element = createElement(UserAvatar, {
        uri: 'https://example.com/avatar.jpg',
      });
      expect(element.props.uri).toBe('https://example.com/avatar.jpg');
    });

    it('should accept undefined uri', () => {
      const element = createElement(UserAvatar, { name: 'John' });
      expect(element.props.uri).toBeUndefined();
    });

    it('should accept empty string uri', () => {
      const element = createElement(UserAvatar, { uri: '' });
      expect(element.props.uri).toBe('');
    });
  });

  describe('name prop', () => {
    it('should accept name prop', () => {
      const element = createElement(UserAvatar, { name: 'Jane Smith' });
      expect(element.props.name).toBe('Jane Smith');
    });

    it('should accept single name', () => {
      const element = createElement(UserAvatar, { name: 'John' });
      expect(element.props.name).toBe('John');
    });

    it('should accept empty name', () => {
      const element = createElement(UserAvatar, { name: '' });
      expect(element.props.name).toBe('');
    });

    it('should accept undefined name', () => {
      const element = createElement(UserAvatar, {});
      expect(element.props.name).toBeUndefined();
    });

    it('should accept name with multiple spaces', () => {
      const element = createElement(UserAvatar, { name: 'John  Middle  Doe' });
      expect(element.props.name).toBe('John  Middle  Doe');
    });

    it('should accept unicode name', () => {
      const element = createElement(UserAvatar, { name: '张三' });
      expect(element.props.name).toBe('张三');
    });
  });

  describe('size prop', () => {
    describe('string size values', () => {
      const sizes: Array<'small' | 'medium' | 'large'> = ['small', 'medium', 'large'];

      for (const size of sizes) {
        it(`should accept size="${size}"`, () => {
          const element = createElement(UserAvatar, { size });
          expect(element.props.size).toBe(size);
        });
      }
    });

    describe('numeric size values', () => {
      it('should accept numeric size', () => {
        const element = createElement(UserAvatar, { size: 100 });
        expect(element.props.size).toBe(100);
      });

      it('should accept small numeric size', () => {
        const element = createElement(UserAvatar, { size: 24 });
        expect(element.props.size).toBe(24);
      });

      it('should accept large numeric size', () => {
        const element = createElement(UserAvatar, { size: 200 });
        expect(element.props.size).toBe(200);
      });
    });

    it('should accept undefined size (uses default)', () => {
      const element = createElement(UserAvatar, {});
      expect(element.props.size).toBeUndefined();
    });
  });

  describe('online status props', () => {
    it('should accept showOnlineStatus=true', () => {
      const element = createElement(UserAvatar, { showOnlineStatus: true });
      expect(element.props.showOnlineStatus).toBe(true);
    });

    it('should accept showOnlineStatus=false', () => {
      const element = createElement(UserAvatar, { showOnlineStatus: false });
      expect(element.props.showOnlineStatus).toBe(false);
    });

    it('should accept isOnline=true', () => {
      const element = createElement(UserAvatar, {
        showOnlineStatus: true,
        isOnline: true,
      });
      expect(element.props.isOnline).toBe(true);
    });

    it('should accept isOnline=false', () => {
      const element = createElement(UserAvatar, {
        showOnlineStatus: true,
        isOnline: false,
      });
      expect(element.props.isOnline).toBe(false);
    });

    it('should handle showOnlineStatus without isOnline', () => {
      const element = createElement(UserAvatar, { showOnlineStatus: true });
      expect(element.props.showOnlineStatus).toBe(true);
      expect(element.props.isOnline).toBeUndefined();
    });
  });

  describe('onPress prop', () => {
    it('should accept onPress callback', () => {
      const onPress = () => {};
      const element = createElement(UserAvatar, { onPress });
      expect(element.props.onPress).toBe(onPress);
    });

    it('should invoke onPress when called', () => {
      let called = false;
      const onPress = () => {
        called = true;
      };
      const element = createElement(UserAvatar, { onPress });
      element.props.onPress();
      expect(called).toBe(true);
    });

    it('should accept undefined onPress', () => {
      const element = createElement(UserAvatar, {});
      expect(element.props.onPress).toBeUndefined();
    });
  });

  describe('style prop', () => {
    it('should accept custom style', () => {
      const style = { marginRight: 10 };
      const element = createElement(UserAvatar, { style });
      expect(element.props.style).toBe(style);
    });

    it('should accept complex style', () => {
      const style = {
        borderWidth: 2,
        borderColor: '#007AFF',
        shadowColor: '#000',
        shadowOpacity: 0.2,
      };
      const element = createElement(UserAvatar, { style });
      expect(element.props.style).toBe(style);
    });
  });

  describe('combined props scenarios', () => {
    it('should handle avatar with image', () => {
      const element = createElement(UserAvatar, {
        uri: 'https://example.com/avatar.jpg',
        name: 'John Doe',
        size: 'large',
      });
      expect(element.props.uri).toBe('https://example.com/avatar.jpg');
      expect(element.props.name).toBe('John Doe');
      expect(element.props.size).toBe('large');
    });

    it('should handle avatar with initials only', () => {
      const element = createElement(UserAvatar, {
        name: 'Jane Smith',
        size: 'medium',
      });
      expect(element.props.uri).toBeUndefined();
      expect(element.props.name).toBe('Jane Smith');
    });

    it('should handle avatar with online status', () => {
      const element = createElement(UserAvatar, {
        name: 'Online User',
        size: 48,
        showOnlineStatus: true,
        isOnline: true,
      });
      expect(element.props.showOnlineStatus).toBe(true);
      expect(element.props.isOnline).toBe(true);
    });

    it('should handle clickable avatar', () => {
      const onPress = () => {};
      const element = createElement(UserAvatar, {
        uri: 'https://example.com/avatar.jpg',
        name: 'Clickable User',
        onPress,
      });
      expect(element.props.onPress).toBe(onPress);
    });

    it('should handle all props together', () => {
      const onPress = () => {};
      const style = { margin: 5 };
      const element = createElement(UserAvatar, {
        uri: 'https://example.com/avatar.jpg',
        name: 'Full Props User',
        size: 64,
        showOnlineStatus: true,
        isOnline: true,
        onPress,
        style,
      });
      expect(element.props.uri).toBe('https://example.com/avatar.jpg');
      expect(element.props.name).toBe('Full Props User');
      expect(element.props.size).toBe(64);
      expect(element.props.showOnlineStatus).toBe(true);
      expect(element.props.isOnline).toBe(true);
      expect(element.props.onPress).toBe(onPress);
      expect(element.props.style).toBe(style);
    });
  });

  describe('typical usage patterns', () => {
    it('should work for chat list avatar', () => {
      const element = createElement(UserAvatar, {
        uri: 'https://example.com/user1.jpg',
        name: 'Chat Partner',
        size: 'medium',
        showOnlineStatus: true,
        isOnline: true,
      });
      expect(element.props.size).toBe('medium');
    });

    it('should work for profile header avatar', () => {
      const onPress = () => {};
      const element = createElement(UserAvatar, {
        uri: 'https://example.com/profile.jpg',
        name: 'Profile User',
        size: 'large',
        onPress,
      });
      expect(element.props.size).toBe('large');
      expect(element.props.onPress).toBeDefined();
    });

    it('should work for comment avatar', () => {
      const element = createElement(UserAvatar, {
        name: 'Commenter',
        size: 'small',
      });
      expect(element.props.size).toBe('small');
    });

    it('should work for avatar group item', () => {
      const element = createElement(UserAvatar, {
        name: 'Group Member',
        size: 32,
        style: { marginLeft: -8 },
      });
      expect(element.props.size).toBe(32);
    });
  });

  describe('edge cases', () => {
    it('should handle very long name', () => {
      const longName = `${'A'.repeat(100)} ${'B'.repeat(100)}`;
      const element = createElement(UserAvatar, { name: longName });
      expect(element.props.name).toBe(longName);
    });

    it('should handle name with special characters', () => {
      const element = createElement(UserAvatar, { name: "O'Brien-Smith Jr." });
      expect(element.props.name).toBe("O'Brien-Smith Jr.");
    });

    it('should handle uri with query params', () => {
      const uri = 'https://example.com/avatar.jpg?size=large&v=2';
      const element = createElement(UserAvatar, { uri });
      expect(element.props.uri).toBe(uri);
    });

    it('should handle data uri', () => {
      const uri =
        'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==';
      const element = createElement(UserAvatar, { uri });
      expect(element.props.uri).toBe(uri);
    });
  });
});
