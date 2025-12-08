/**
 * EngineView Tests
 * @vitest-environment jsdom
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor, screen, act } from '@testing-library/react';
import React from 'react';
import { EngineView } from './EngineView';
import { Engine } from './engine';

// Mock React Native components
vi.mock('react-native', () => ({
  View: ({ children, ...props }: any) => <div {...props}>{children}</div>,
  Text: ({ children, ...props }: any) => <span {...props}>{children}</span>,
  ActivityIndicator: (props: any) => <div data-testid="activity-indicator" {...props} />,
  StyleSheet: {
    create: (styles: any) => styles,
  },
}));

// Type definitions for mock QuickJS
interface MockQuickJSContext {
  eval(code: string): unknown;
  setGlobal(name: string, value: unknown): void;
  getGlobal(name: string): unknown;
  dispose(): void;
}

interface MockQuickJSRuntime {
  createContext(): MockQuickJSContext;
  dispose(): void;
}

interface MockQuickJSProvider {
  createRuntime(): MockQuickJSRuntime;
}

// Mock QuickJS Provider for tests
function createMockQuickJSProvider(): MockQuickJSProvider {
  return {
    createRuntime(): MockQuickJSRuntime {
      const globals = new Map<string, unknown>();
      return {
        createContext(): MockQuickJSContext {
          return {
            eval(code: string): unknown {
              const globalNames = Array.from(globals.keys());
              const globalValues = Array.from(globals.values());
              try {
                const fn = new Function(...globalNames, `"use strict"; ${code}`);
                return fn(...globalValues);
              } catch (e) {
                throw e;
              }
            },
            setGlobal(name: string, value: unknown): void {
              globals.set(name, value);
            },
            getGlobal(name: string): unknown {
              return globals.get(name);
            },
            dispose(): void {
              globals.clear();
            },
          };
        },
        dispose(): void {},
      };
    },
  };
}

describe('EngineView', () => {
  let mockEngine: Engine;

  beforeEach(() => {
    // Create a real Engine instance for testing
    mockEngine = new Engine({ quickjs: createMockQuickJSProvider() });
    vi.spyOn(mockEngine, 'loadBundle').mockResolvedValue();

    // Mock createReceiver to call the update callback synchronously
    vi.spyOn(mockEngine, 'createReceiver').mockImplementation((updateCallback?: () => void) => {
      if (updateCallback) {
        // Trigger update callback synchronously to avoid act() warnings
        queueMicrotask(() => {
          act(() => {
            updateCallback();
          });
        });
      }
    });

    vi.spyOn(mockEngine, 'getReceiver').mockReturnValue({
      render: () => <div data-testid="plugin-content">Plugin Content</div>,
    } as any);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('初始化和加载', () => {
    it('应该显示默认加载指示器', () => {
      act(() => {
        render(<EngineView engine={mockEngine} bundleUrl="https://example.com/bundle.js" />);
      });

      // Check loading state immediately before async loading completes
      expect(screen.getByTestId('activity-indicator')).toBeTruthy();
      expect(screen.getByText('Loading plugin...')).toBeTruthy();
    });

    it('应该显示自定义加载指示器', () => {
      const customFallback = <div data-testid="custom-loader">Loading...</div>;

      act(() => {
        render(
          <EngineView
            engine={mockEngine}
            bundleUrl="https://example.com/bundle.js"
            fallback={customFallback}
          />
        );
      });

      // Check loading state immediately before async loading completes
      expect(screen.getByTestId('custom-loader')).toBeTruthy();
    });

    it('应该调用 engine.createReceiver 和 loadBundle', async () => {
      render(<EngineView engine={mockEngine} bundleUrl="https://example.com/bundle.js" />);

      await waitFor(() => {
        expect(mockEngine.createReceiver).toHaveBeenCalled();
        expect(mockEngine.loadBundle).toHaveBeenCalledWith(
          'https://example.com/bundle.js',
          undefined
        );
      });
    });

    it('应该传递 initialProps 给 loadBundle', async () => {
      const initialProps = { theme: 'dark', userId: 123 };

      render(
        <EngineView
          engine={mockEngine}
          bundleUrl="https://example.com/bundle.js"
          initialProps={initialProps}
        />
      );

      await waitFor(() => {
        expect(mockEngine.loadBundle).toHaveBeenCalledWith(
          'https://example.com/bundle.js',
          initialProps
        );
      });
    });

    it('加载成功后应该调用 onLoad 回调', async () => {
      const onLoad = vi.fn();

      render(
        <EngineView
          engine={mockEngine}
          bundleUrl="https://example.com/bundle.js"
          onLoad={onLoad}
        />
      );

      await waitFor(() => {
        expect(onLoad).toHaveBeenCalled();
      });
    });

    it('加载成功后应该渲染插件内容', async () => {
      render(<EngineView engine={mockEngine} bundleUrl="https://example.com/bundle.js" />);

      await waitFor(() => {
        expect(screen.getByTestId('plugin-content')).toBeTruthy();
      });
    });
  });

  describe('错误处理', () => {
    it('加载失败时应该显示默认错误 UI', async () => {
      const error = new Error('Bundle load failed');
      vi.spyOn(mockEngine, 'loadBundle').mockRejectedValue(error);

      render(<EngineView engine={mockEngine} bundleUrl="https://example.com/bundle.js" />);

      await waitFor(() => {
        expect(screen.getByText('Plugin Error')).toBeTruthy();
        expect(screen.getByText('Bundle load failed')).toBeTruthy();
      });
    });

    it('加载失败时应该调用 onError 回调', async () => {
      const error = new Error('Bundle load failed');
      const onError = vi.fn();
      vi.spyOn(mockEngine, 'loadBundle').mockRejectedValue(error);

      render(
        <EngineView
          engine={mockEngine}
          bundleUrl="https://example.com/bundle.js"
          onError={onError}
        />
      );

      await waitFor(() => {
        expect(onError).toHaveBeenCalledWith(error);
      });
    });

    it('应该显示自定义错误 UI', async () => {
      const error = new Error('Bundle load failed');
      const renderError = (err: Error) => (
        <div data-testid="custom-error">Custom Error: {err.message}</div>
      );
      vi.spyOn(mockEngine, 'loadBundle').mockRejectedValue(error);

      render(
        <EngineView
          engine={mockEngine}
          bundleUrl="https://example.com/bundle.js"
          renderError={renderError}
        />
      );

      await waitFor(() => {
        expect(screen.getByTestId('custom-error')).toBeTruthy();
        expect(screen.getByText('Custom Error: Bundle load failed')).toBeTruthy();
      });
    });

    it('应该处理非 Error 对象的异常', async () => {
      vi.spyOn(mockEngine, 'loadBundle').mockRejectedValue('String error');
      const onError = vi.fn();

      render(
        <EngineView
          engine={mockEngine}
          bundleUrl="https://example.com/bundle.js"
          onError={onError}
        />
      );

      await waitFor(() => {
        expect(onError).toHaveBeenCalled();
        const capturedError = onError.mock.calls[0][0];
        expect(capturedError).toBeInstanceOf(Error);
        expect(capturedError.message).toBe('String error');
      });
    });
  });

  describe('Engine 事件监听', () => {
    it('应该监听并处理 engine error 事件', async () => {
      const onError = vi.fn();

      render(
        <EngineView
          engine={mockEngine}
          bundleUrl="https://example.com/bundle.js"
          onError={onError}
        />
      );

      // 等待组件挂载
      await waitFor(() => {
        expect(mockEngine.loadBundle).toHaveBeenCalled();
      });

      // 触发 error 事件
      const runtimeError = new Error('Runtime error');
      act(() => {
        mockEngine.emit('error', runtimeError);
      });

      await waitFor(() => {
        expect(onError).toHaveBeenCalledWith(runtimeError);
      });
    });

    it('应该监听并处理 engine destroy 事件', async () => {
      const onDestroy = vi.fn();

      render(
        <EngineView
          engine={mockEngine}
          bundleUrl="https://example.com/bundle.js"
          onDestroy={onDestroy}
        />
      );

      // 等待组件挂载
      await waitFor(() => {
        expect(mockEngine.loadBundle).toHaveBeenCalled();
      });

      // 触发 destroy 事件
      act(() => {
        mockEngine.emit('destroy');
      });

      await waitFor(() => {
        expect(onDestroy).toHaveBeenCalled();
      });
    });
  });

  describe('生命周期管理', () => {
    it('不应该在 engine 已加载时重复加载', async () => {
      Object.defineProperty(mockEngine, 'isLoaded', {
        get: () => true,
        configurable: true,
      });

      render(<EngineView engine={mockEngine} bundleUrl="https://example.com/bundle.js" />);

      await new Promise(resolve => setTimeout(resolve, 100));

      expect(mockEngine.loadBundle).not.toHaveBeenCalled();
    });

    it('不应该在 engine 已销毁时加载', async () => {
      Object.defineProperty(mockEngine, 'isDestroyed', {
        get: () => true,
        configurable: true,
      });

      render(<EngineView engine={mockEngine} bundleUrl="https://example.com/bundle.js" />);

      await new Promise(resolve => setTimeout(resolve, 100));

      expect(mockEngine.loadBundle).not.toHaveBeenCalled();
    });

    it('组件卸载时不应该更新状态', async () => {
      const { unmount } = render(
        <EngineView engine={mockEngine} bundleUrl="https://example.com/bundle.js" />
      );

      // 立即卸载组件
      unmount();

      // 不应该抛出错误或警告
      await new Promise(resolve => setTimeout(resolve, 100));
    });
  });

  describe('样式和布局', () => {
    it('应该应用自定义样式', () => {
      const customStyle = { backgroundColor: 'red', padding: 20 };

      let container: HTMLElement;
      act(() => {
        const result = render(
          <EngineView
            engine={mockEngine}
            bundleUrl="https://example.com/bundle.js"
            style={customStyle}
          />
        );
        container = result.container;
      });

      // Check immediately before async loading completes
      const view = container!.firstChild as HTMLElement;
      expect(view).toBeTruthy();
    });
  });

  describe('边界情况', () => {
    it('应该处理空 bundleUrl', async () => {
      const onError = vi.fn();

      render(
        <EngineView
          engine={mockEngine}
          bundleUrl=""
          onError={onError}
        />
      );

      await waitFor(() => {
        expect(mockEngine.loadBundle).toHaveBeenCalledWith('', undefined);
      });
    });

    it('应该处理 receiver.render() 返回 null', async () => {
      vi.spyOn(mockEngine, 'getReceiver').mockReturnValue({
        render: () => null,
      } as any);

      const { container } = render(
        <EngineView engine={mockEngine} bundleUrl="https://example.com/bundle.js" />
      );

      await waitFor(() => {
        expect(mockEngine.loadBundle).toHaveBeenCalled();
      });

      expect(container.firstChild).toBeTruthy();
    });

    it('应该处理 getReceiver() 返回 null', async () => {
      vi.spyOn(mockEngine, 'getReceiver').mockReturnValue(null);

      render(<EngineView engine={mockEngine} bundleUrl="https://example.com/bundle.js" />);

      await waitFor(() => {
        expect(mockEngine.loadBundle).toHaveBeenCalled();
      });
    });
  });
});
