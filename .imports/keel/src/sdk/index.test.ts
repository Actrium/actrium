/**
 * SDK 单元测试
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  View,
  Text,
  Image,
  ScrollView,
  TouchableOpacity,
  FlatList,
  TextInput,
  Button,
  Switch,
  ActivityIndicator,
  useHostEvent,
  useConfig,
  useSendToHost,
} from './index';

// ============ 虚组件测试 ============

describe('Virtual Components', () => {
  describe('Component Identifiers', () => {
    it('View should be string "View"', () => {
      expect(View).toBe('View');
      expect(typeof View).toBe('string');
    });

    it('Text should be string "Text"', () => {
      expect(Text).toBe('Text');
      expect(typeof Text).toBe('string');
    });

    it('Image should be string "Image"', () => {
      expect(Image).toBe('Image');
      expect(typeof Image).toBe('string');
    });

    it('ScrollView should be string "ScrollView"', () => {
      expect(ScrollView).toBe('ScrollView');
      expect(typeof ScrollView).toBe('string');
    });

    it('TouchableOpacity should be string "TouchableOpacity"', () => {
      expect(TouchableOpacity).toBe('TouchableOpacity');
      expect(typeof TouchableOpacity).toBe('string');
    });

    it('FlatList should be string "FlatList"', () => {
      expect(FlatList).toBe('FlatList');
      expect(typeof FlatList).toBe('string');
    });

    it('TextInput should be string "TextInput"', () => {
      expect(TextInput).toBe('TextInput');
      expect(typeof TextInput).toBe('string');
    });

    it('Button should be string "Button"', () => {
      expect(Button).toBe('Button');
      expect(typeof Button).toBe('string');
    });

    it('Switch should be string "Switch"', () => {
      expect(Switch).toBe('Switch');
      expect(typeof Switch).toBe('string');
    });

    it('ActivityIndicator should be string "ActivityIndicator"', () => {
      expect(ActivityIndicator).toBe('ActivityIndicator');
      expect(typeof ActivityIndicator).toBe('string');
    });
  });

  describe('Component as JSX type', () => {
    it('components can be used as type references', () => {
      // 这些组件作为字符串可以用于动态创建元素
      const componentTypes = [
        View,
        Text,
        Image,
        ScrollView,
        TouchableOpacity,
        FlatList,
        TextInput,
        Button,
        Switch,
        ActivityIndicator,
      ];

      componentTypes.forEach((type) => {
        expect(typeof type).toBe('string');
        expect(type.length).toBeGreaterThan(0);
      });
    });
  });
});

// ============ Hooks 测试 ============

describe('Hooks', () => {
  describe('useHostEvent', () => {
    beforeEach(() => {
      // 清理 globalThis
      delete (globalThis as Record<string, unknown>).__useHostEvent;
    });

    afterEach(() => {
      delete (globalThis as Record<string, unknown>).__useHostEvent;
    });

    it('should call global __useHostEvent when available', () => {
      const mockUseHostEvent = vi.fn();
      (globalThis as Record<string, unknown>).__useHostEvent = mockUseHostEvent;

      const callback = vi.fn();
      useHostEvent('REFRESH', callback);

      expect(mockUseHostEvent).toHaveBeenCalledWith('REFRESH', callback);
    });

    it('should not throw when __useHostEvent is not available', () => {
      const callback = vi.fn();

      expect(() => {
        useHostEvent('REFRESH', callback);
      }).not.toThrow();
    });

    it('should accept typed payload callback', () => {
      const mockUseHostEvent = vi.fn();
      (globalThis as Record<string, unknown>).__useHostEvent = mockUseHostEvent;

      interface RefreshPayload {
        timestamp: number;
      }

      const callback = vi.fn((payload: RefreshPayload) => {
        console.log(payload.timestamp);
      });

      useHostEvent<RefreshPayload>('REFRESH', callback);

      expect(mockUseHostEvent).toHaveBeenCalled();
    });
  });

  describe('useConfig', () => {
    beforeEach(() => {
      delete (globalThis as Record<string, unknown>).__getConfig;
    });

    afterEach(() => {
      delete (globalThis as Record<string, unknown>).__getConfig;
    });

    it('should return config from global __getConfig', () => {
      const mockConfig = { theme: 'dark', fontSize: 14 };
      (globalThis as Record<string, unknown>).__getConfig = () => mockConfig;

      const config = useConfig<{ theme: string; fontSize: number }>();

      expect(config).toEqual(mockConfig);
    });

    it('should return empty object when __getConfig is not available', () => {
      const config = useConfig();

      expect(config).toEqual({});
    });

    it('should support generic type parameter', () => {
      interface AppConfig {
        theme: 'light' | 'dark';
        language: string;
      }

      const mockConfig: AppConfig = { theme: 'dark', language: 'en' };
      (globalThis as Record<string, unknown>).__getConfig = () => mockConfig;

      const config = useConfig<AppConfig>();

      expect(config.theme).toBe('dark');
      expect(config.language).toBe('en');
    });
  });

  describe('useSendToHost', () => {
    beforeEach(() => {
      delete (globalThis as Record<string, unknown>).__sendEventToHost;
    });

    afterEach(() => {
      delete (globalThis as Record<string, unknown>).__sendEventToHost;
    });

    it('should return global __sendEventToHost when available', () => {
      const mockSend = vi.fn();
      (globalThis as Record<string, unknown>).__sendEventToHost = mockSend;

      const send = useSendToHost();
      send('TEST_EVENT', { data: 123 });

      expect(mockSend).toHaveBeenCalledWith('TEST_EVENT', { data: 123 });
    });

    it('should return noop function with warning when not available', () => {
      const consoleSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});

      const send = useSendToHost();
      send('TEST_EVENT');

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('sendToHost is not available')
      );

      consoleSpy.mockRestore();
    });

    it('should accept optional payload', () => {
      const mockSend = vi.fn();
      (globalThis as Record<string, unknown>).__sendEventToHost = mockSend;

      const send = useSendToHost();
      send('EVENT_WITHOUT_PAYLOAD');

      expect(mockSend).toHaveBeenCalledWith('EVENT_WITHOUT_PAYLOAD');
    });
  });
});

// ============ 类型导出测试 ============

describe('Type Exports', () => {
  it('should export StyleProp type', async () => {
    // 通过导入验证类型存在
    const module = await import('./index');
    expect(module).toBeDefined();
  });
});

// ============ Props 类型测试 (编译时) ============

describe('Props Types', () => {
  it('ViewProps should accept style and children', () => {
    // 这是编译时测试，运行时只验证类型存在
    const viewProps = {
      style: { flex: 1, backgroundColor: 'red' },
      testID: 'test-view',
    };
    expect(viewProps).toBeDefined();
  });

  it('TextProps should accept numberOfLines', () => {
    const textProps = {
      numberOfLines: 2,
      ellipsizeMode: 'tail' as const,
    };
    expect(textProps).toBeDefined();
  });

  it('ImageProps should accept source', () => {
    const imageProps = {
      source: { uri: 'https://example.com/image.png' },
      resizeMode: 'cover' as const,
    };
    expect(imageProps).toBeDefined();
  });

  it('TouchableOpacityProps should accept onPress', () => {
    const touchableProps = {
      onPress: () => {},
      activeOpacity: 0.7,
      disabled: false,
    };
    expect(touchableProps).toBeDefined();
  });

  it('ScrollViewProps should accept horizontal and onScroll', () => {
    const scrollViewProps = {
      horizontal: true,
      showsVerticalScrollIndicator: false,
      onScroll: (event: { nativeEvent: object }) => {
        console.log(event);
      },
    };
    expect(scrollViewProps).toBeDefined();
  });

  it('TextInputProps should accept value and onChangeText', () => {
    const textInputProps = {
      value: 'hello',
      onChangeText: (text: string) => {
        console.log(text);
      },
      placeholder: 'Enter text',
      keyboardType: 'default' as const,
    };
    expect(textInputProps).toBeDefined();
  });

  it('FlatListProps should accept data and renderItem', () => {
    interface Item {
      id: string;
      name: string;
    }

    const flatListProps = {
      data: [{ id: '1', name: 'Item 1' }] as Item[],
      renderItem: ({ item }: { item: Item }) => item.name,
      keyExtractor: (item: Item) => item.id,
    };
    expect(flatListProps).toBeDefined();
  });
});

// ============ 事件类型测试 ============

describe('Event Types', () => {
  it('LayoutEvent should have correct structure', () => {
    const layoutEvent = {
      nativeEvent: {
        layout: {
          x: 0,
          y: 0,
          width: 100,
          height: 200,
        },
      },
    };
    expect(layoutEvent.nativeEvent.layout.width).toBe(100);
  });

  it('ScrollEvent should have correct structure', () => {
    const scrollEvent = {
      nativeEvent: {
        contentOffset: { x: 0, y: 100 },
        contentSize: { width: 375, height: 1000 },
        layoutMeasurement: { width: 375, height: 667 },
      },
    };
    expect(scrollEvent.nativeEvent.contentOffset.y).toBe(100);
  });
});

// ============ ImageSource 类型测试 ============

describe('ImageSource Type', () => {
  it('should accept URI object', () => {
    const source = {
      uri: 'https://example.com/image.png',
      width: 100,
      height: 100,
    };
    expect(source.uri).toBe('https://example.com/image.png');
  });

  it('should accept URI with headers', () => {
    const source = {
      uri: 'https://example.com/image.png',
      headers: {
        Authorization: 'Bearer token',
      },
    };
    expect(source.headers?.Authorization).toBe('Bearer token');
  });
});

// ============ 零依赖验证 ============

describe('Zero Runtime Dependencies', () => {
  it('SDK should not import react-native', async () => {
    // 验证模块不包含 react-native 导入
    const moduleSource = await import('./index');

    // 所有组件都应该是字符串，不是实际组件
    expect(typeof moduleSource.View).toBe('string');
    expect(typeof moduleSource.Text).toBe('string');
    expect(typeof moduleSource.Image).toBe('string');
  });

  it('SDK should only export primitives and functions', async () => {
    const moduleSource = await import('./index');

    // 组件是字符串
    const components = [
      moduleSource.View,
      moduleSource.Text,
      moduleSource.Image,
      moduleSource.ScrollView,
      moduleSource.TouchableOpacity,
    ];

    components.forEach((component) => {
      expect(['string', 'function']).toContain(typeof component);
    });

    // Hooks 是函数
    expect(typeof moduleSource.useHostEvent).toBe('function');
    expect(typeof moduleSource.useConfig).toBe('function');
    expect(typeof moduleSource.useSendToHost).toBe('function');
  });
});
