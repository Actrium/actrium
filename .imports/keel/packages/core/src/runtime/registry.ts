/**
 * Component Registry
 *
 * Manages renderable component mappings, implementing whitelist security mechanism
 */

import type React from 'react';

/**
 * Component type
 */
export type ComponentType = React.ComponentType<Record<string, unknown>>;

/**
 * Component map
 */
export type ComponentMap = Record<string, ComponentType>;

/**
 * Component registry
 */
export class ComponentRegistry {
  private components = new Map<string, ComponentType>();

  /**
   * Register a single component
   *
   * @param name Component name
   * @param component Component implementation
   */
  register(name: string, component: ComponentType): void {
    if (this.components.has(name)) {
      console.warn(`[rill] Component "${name}" is already registered, overwriting`);
    }
    this.components.set(name, component);
  }

  /**
   * Register multiple components
   *
   * @param map Component mapping object
   */
  registerAll(map: ComponentMap): void {
    Object.entries(map).forEach(([name, component]) => {
      this.register(name, component);
    });
  }

  /**
   * Get a component
   *
   * @param name Component name
   * @returns Component implementation or undefined
   */
  get(name: string): ComponentType | undefined {
    return this.components.get(name);
  }

  /**
   * Check if a component is registered
   *
   * @param name Component name
   */
  has(name: string): boolean {
    return this.components.has(name);
  }

  /**
   * Unregister a component
   *
   * @param name Component name
   */
  unregister(name: string): boolean {
    return this.components.delete(name);
  }

  /**
   * Get all registered component names
   */
  getRegisteredNames(): string[] {
    return Array.from(this.components.keys());
  }

  /**
   * Clear all registrations
   */
  clear(): void {
    this.components.clear();
  }

  /**
   * Get registered component count
   */
  get size(): number {
    return this.components.size;
  }
}

/**
 * Create a registry with default components
 */
export function createRegistry(): ComponentRegistry {
  return new ComponentRegistry();
}
