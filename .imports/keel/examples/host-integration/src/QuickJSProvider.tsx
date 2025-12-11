/**
 * QuickJS Provider Configuration Example
 *
 * In production applications, you need to choose the appropriate QuickJS provider based on platform:
 * - react-native-quick-js (React Native)
 * - quickjs-emscripten (Web)
 * - Custom implementation
 */

import type { QuickJSProvider } from '@rill/core';

/**
 * Create QuickJS Provider
 *
 * Note: This is an example implementation that needs to be replaced based on the actual environment
 */
export function createQuickJSProvider(): QuickJSProvider {
  // Example: Using react-native-quick-js
  // const { QuickJS } = require('react-native-quick-js');
  // return QuickJS;

  // Example: Using quickjs-emscripten (Web)
  // import { getQuickJS } from 'quickjs-emscripten';
  // const QuickJS = await getQuickJS();
  // return QuickJS;

  // Development environment mock (for demonstration only)
  console.warn(
    '[QuickJSProvider] Using mock provider, please replace with actual QuickJS provider in production'
  );

  return {
    createRuntime() {
      const globals = new Map<string, any>();

      return {
        createContext() {
          return {
            eval(code: string) {
              const globalNames = Array.from(globals.keys());
              const globalValues = Array.from(globals.values());
              try {
                const fn = new Function(...globalNames, `"use strict"; ${code}`);
                return fn(...globalValues);
              } catch (e) {
                throw e;
              }
            },
            setGlobal(name: string, value: any) {
              globals.set(name, value);
            },
            getGlobal(name: string) {
              return globals.get(name);
            },
            dispose() {
              globals.clear();
            },
          };
        },
        dispose() {},
      };
    },
  };
}

/**
 * Production Environment Example - React Native
 */
export function createReactNativeQuickJSProvider(): QuickJSProvider {
  try {
    const { QuickJS } = require('react-native-quick-js');
    return QuickJS;
  } catch (error) {
    console.error('Failed to load react-native-quick-js:', error);
    throw new Error(
      'react-native-quick-js is required. Please install it: npm install react-native-quick-js'
    );
  }
}

/**
 * Production Environment Example - Web (requires async)
 */
export async function createWebQuickJSProvider(): Promise<QuickJSProvider> {
  try {
    const { getQuickJS } = await import('quickjs-emscripten');
    return await getQuickJS();
  } catch (error) {
    console.error('Failed to load quickjs-emscripten:', error);
    throw new Error(
      'quickjs-emscripten is required. Please install it: npm install quickjs-emscripten'
    );
  }
}
