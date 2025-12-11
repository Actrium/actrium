/**
 * ComponentRegistry unit tests
 */

import { describe, it, expect, beforeEach, mock, spyOn } from 'bun:test';
import { ComponentRegistry, createRegistry } from './registry';
import React from 'react';

// Mock 组件
const MockView: React.FC<{ children?: React.ReactNode }> = ({ children }) =>
  React.createElement('View', null, children);

const MockText: React.FC<{ children?: React.ReactNode }> = ({ children }) =>
  React.createElement('Text', null, children);

const MockImage: React.FC<{ source?: string }> = () =>
  React.createElement('Image');

const MockCustomComponent: React.FC<{ data?: unknown[] }> = () =>
  React.createElement('CustomComponent');

describe('ComponentRegistry', () => {
  let registry: ComponentRegistry;

  beforeEach(() => {
    registry = new ComponentRegistry();
  });

  describe('register', () => {
    it('should register a single component', () => {
      registry.register('View', MockView);

      expect(registry.has('View')).toBe(true);
      expect(registry.get('View')).toBe(MockView);
      expect(registry.size).toBe(1);
    });

    it('should overwrite existing component with warning', () => {
      if (!globalThis.console?.warn) {
        // Skip this test if console.warn is not available
        return;
      }

      const consoleSpy = spyOn(console, 'warn').mockImplementation(() => {});

      registry.register('View', MockView);
      registry.register('View', MockText);

      expect(registry.get('View')).toBe(MockText);
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Component "View" is already registered')
      );

      consoleSpy.mockRestore();
    });

    it('should register multiple different components', () => {
      registry.register('View', MockView);
      registry.register('Text', MockText);
      registry.register('Image', MockImage);

      expect(registry.size).toBe(3);
      expect(registry.get('View')).toBe(MockView);
      expect(registry.get('Text')).toBe(MockText);
      expect(registry.get('Image')).toBe(MockImage);
    });
  });

  describe('registerAll', () => {
    it('should register multiple components at once', () => {
      registry.registerAll({
        View: MockView,
        Text: MockText,
        Image: MockImage,
      });

      expect(registry.size).toBe(3);
      expect(registry.has('View')).toBe(true);
      expect(registry.has('Text')).toBe(true);
      expect(registry.has('Image')).toBe(true);
    });

    it('should handle empty object', () => {
      registry.registerAll({});

      expect(registry.size).toBe(0);
    });

    it('should merge with existing registrations', () => {
      registry.register('View', MockView);
      registry.registerAll({
        Text: MockText,
        Image: MockImage,
      });

      expect(registry.size).toBe(3);
    });
  });

  describe('get', () => {
    it('should return registered component', () => {
      registry.register('View', MockView);

      expect(registry.get('View')).toBe(MockView);
    });

    it('should return undefined for non-existent component', () => {
      expect(registry.get('NonExistent')).toBeUndefined();
    });

    it('should be case-sensitive', () => {
      registry.register('View', MockView);

      expect(registry.get('view')).toBeUndefined();
      expect(registry.get('VIEW')).toBeUndefined();
      expect(registry.get('View')).toBe(MockView);
    });
  });

  describe('has', () => {
    it('should return true for registered component', () => {
      registry.register('View', MockView);

      expect(registry.has('View')).toBe(true);
    });

    it('should return false for non-existent component', () => {
      expect(registry.has('NonExistent')).toBe(false);
    });
  });

  describe('unregister', () => {
    it('should unregister a component', () => {
      registry.register('View', MockView);
      expect(registry.has('View')).toBe(true);

      const result = registry.unregister('View');

      expect(result).toBe(true);
      expect(registry.has('View')).toBe(false);
      expect(registry.size).toBe(0);
    });

    it('should return false when unregistering non-existent component', () => {
      const result = registry.unregister('NonExistent');

      expect(result).toBe(false);
    });
  });

  describe('getRegisteredNames', () => {
    it('should return empty array when no components registered', () => {
      expect(registry.getRegisteredNames()).toEqual([]);
    });

    it('should return all registered component names', () => {
      registry.registerAll({
        View: MockView,
        Text: MockText,
        Image: MockImage,
      });

      const names = registry.getRegisteredNames();

      expect(names).toHaveLength(3);
      expect(names).toContain('View');
      expect(names).toContain('Text');
      expect(names).toContain('Image');
    });
  });

  describe('clear', () => {
    it('should remove all registered components', () => {
      registry.registerAll({
        View: MockView,
        Text: MockText,
        Image: MockImage,
      });
      expect(registry.size).toBe(3);

      registry.clear();

      expect(registry.size).toBe(0);
      expect(registry.has('View')).toBe(false);
      expect(registry.has('Text')).toBe(false);
      expect(registry.has('Image')).toBe(false);
    });
  });

  describe('size', () => {
    it('should return 0 for empty registry', () => {
      expect(registry.size).toBe(0);
    });

    it('should return correct count after registrations', () => {
      registry.register('View', MockView);
      expect(registry.size).toBe(1);

      registry.register('Text', MockText);
      expect(registry.size).toBe(2);

      registry.unregister('View');
      expect(registry.size).toBe(1);
    });
  });
});

describe('createRegistry', () => {
  it('should create a new ComponentRegistry instance', () => {
    const registry = createRegistry();

    expect(registry).toBeInstanceOf(ComponentRegistry);
    expect(registry.size).toBe(0);
  });

  it('should create independent instances', () => {
    const registry1 = createRegistry();
    const registry2 = createRegistry();

    registry1.register('View', MockView);

    expect(registry1.has('View')).toBe(true);
    expect(registry2.has('View')).toBe(false);
  });
});

describe('Component Type Safety', () => {
  let registry: ComponentRegistry;

  beforeEach(() => {
    registry = new ComponentRegistry();
  });

  it('should accept functional components', () => {
    const FunctionalComponent: React.FC = () => React.createElement('div');
    registry.register('Functional', FunctionalComponent);

    expect(registry.get('Functional')).toBe(FunctionalComponent);
  });

  it('should accept components with props', () => {
    interface Props {
      title: string;
      count: number;
    }
    const PropsComponent: React.FC<Props> = ({ title, count }) =>
      React.createElement('div', null, `${title}: ${count}`);

    registry.register('PropsComponent', PropsComponent);

    expect(registry.get('PropsComponent')).toBe(PropsComponent);
  });

  it('should accept components with children', () => {
    const ContainerComponent: React.FC<{ children?: React.ReactNode }> = ({
      children,
    }) => React.createElement('div', null, children);

    registry.register('Container', ContainerComponent);

    expect(registry.get('Container')).toBe(ContainerComponent);
  });
});

describe('White-list Security Model', () => {
  let registry: ComponentRegistry;

  beforeEach(() => {
    registry = new ComponentRegistry();
    // Register allowed components only
    registry.registerAll({
      View: MockView,
      Text: MockText,
    });
  });

  it('should only allow registered components', () => {
    // Registered components available
    expect(registry.get('View')).toBeDefined();
    expect(registry.get('Text')).toBeDefined();

    // Unregistered components unavailable
    expect(registry.get('Script')).toBeUndefined();
    expect(registry.get('eval')).toBeUndefined();
    expect(registry.get('dangerouslySetInnerHTML')).toBeUndefined();
  });

  it('should prevent injection of unauthorized components', () => {
    // Try retrieving unregistered potentially dangerous components
    const dangerousNames = [
      'script',
      'iframe',
      'object',
      'embed',
      '__proto__',
      'constructor',
    ];

    for (const name of dangerousNames) {
      expect(registry.has(name)).toBe(false);
      expect(registry.get(name)).toBeUndefined();
    }
  });
});
