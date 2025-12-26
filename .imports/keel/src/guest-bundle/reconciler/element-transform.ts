/**
 * Guest Element Transformation
 *
 * Handles transformation of Guest React elements to Host-compatible format.
 * - Symbol registry differences between Guest (JSC) and Host (Hermes)
 * - JSI-safe component type transport via registration
 * - Function prop serialization for sandbox components
 */

import type { ReactElement } from 'react';
import React from 'react';
import type { SerializedValue } from '../../bridge';
import type { GuestElement } from '../../let/types';
import { isGuestReactElement } from '../../let/types';
import { serializeProps } from './guest-encoder';

// ============================================
// Type Definitions
// ============================================

/**
 * Sandbox wrapper component interface
 * Marks components that wrap Guest function components
 */
interface SandboxWrapper extends React.FC<Record<string, unknown>> {
  __rillSandboxWrapper: true;
  displayName?: string;
}

/**
 * Guest React element with runtime fields
 * Extends GuestReactElement to access internal markers and fields
 */
interface GuestElementRuntime {
  __rillTypeMarker?: string;
  __rillFragmentType?: string;
  $$typeof?: symbol;
  type?: unknown;
  props?: Record<string, unknown>;
  key?: React.Key | null;
  ref?: unknown;
}

/**
 * Component type reference from sandbox
 */
interface ComponentTypeRef {
  __rillComponentId: string;
  displayName?: string;
}

/**
 * Type guard for component type reference
 */
function isComponentTypeRef(value: unknown): value is ComponentTypeRef {
  return (
    typeof value === 'object' &&
    value !== null &&
    '__rillComponentId' in value &&
    typeof (value as ComponentTypeRef).__rillComponentId === 'string'
  );
}

// ============================================
// Constants
// ============================================

const RILL_ELEMENT_MARKER = '__rill_react_element__';
const RILL_FRAGMENT_MARKER = '__rill_react_fragment__';
const REACT_FRAGMENT_TYPE = Symbol.for('react.fragment');

// ============================================
// Component Type Registry
// ============================================
// JSI-safe function component transport
// In some sandboxes, functions inside returned object graphs lose callability.
// We register function component handles on the Host side and reference them by id.

// biome-ignore lint/complexity/noBannedTypes: WeakMap requires Function type for component tracking
const componentTypeIdByFn = new WeakMap<Function, string>();
// biome-ignore lint/complexity/noBannedTypes: Map stores component functions by ID
const componentTypeFnById = new Map<string, Function>();
const componentTypeOwnerById = new Map<string, string>();
let componentTypeCounter = 0;

export function registerComponentType(fn: unknown, ownerId = 'global'): string | null {
  if (typeof fn !== 'function') return null;
  const existing = componentTypeIdByFn.get(fn);
  if (existing) return existing;

  const id = `cmp_${ownerId}_${++componentTypeCounter}`;
  componentTypeIdByFn.set(fn, id);
  componentTypeFnById.set(id, fn);
  componentTypeOwnerById.set(id, ownerId);
  return id;
}

export function unregisterComponentTypes(ownerId: string): void {
  for (const [id, currentOwner] of componentTypeOwnerById) {
    if (currentOwner !== ownerId) continue;
    componentTypeOwnerById.delete(id);
    const fn = componentTypeFnById.get(id);
    componentTypeFnById.delete(id);
    if (fn) componentTypeIdByFn.delete(fn);
  }
}

// ============================================
// Component Wrapper Cache
// ============================================

// biome-ignore lint/suspicious/noExplicitAny: Component wrapper cache accepts any props type
const componentWrappers = new WeakMap<object, React.ComponentType<any>>();
let hookInstanceCounter = 0;
const hookInstancePrefix = Math.random().toString(36).slice(2, 7);

function getOrCreateWrappedComponent(
  // biome-ignore lint/complexity/noBannedTypes: Function type required for component wrapping
  originalType: Function,
  displayName?: string
  // biome-ignore lint/suspicious/noExplicitAny: Wrapped component accepts any props type
): React.ComponentType<any> {
  const key = originalType as unknown as object;
  if (componentWrappers.has(key)) {
    return componentWrappers.get(key)!;
  }

  const wrapped: SandboxWrapper = Object.assign(
    (props: Record<string, unknown>) => {
      const hookIdRef = React.useRef<string>('');
      if (!hookIdRef.current) {
        hookIdRef.current = `inst_${hookInstancePrefix}_${++hookInstanceCounter}`;
      }
      try {
        // Note: originalType may be a JSI-bridged sandbox function (e.g., JSC Sandbox).
        // Passing Host ReactElement (with Fiber/circular refs) as props back to sandbox
        // can trigger massive `[JSCSandbox] Exception` or system kill.
        // Do shallow copy to avoid sending objects with prototypes/internal fields to sandbox.
        // Note: Function props are already serialized in transformGuestElement before reaching here
        // Reason: Props type unknown until runtime shallow copy and field injection
        const safeProps: unknown =
          props && typeof props === 'object' ? { ...(props as Record<string, unknown>) } : {};
        if (safeProps && typeof safeProps === 'object') {
          (safeProps as Record<string, unknown>).__rillHookInstanceId = hookIdRef.current;
        }

        const result = originalType(safeProps);
        return transformGuestElement(result);
      } catch (err) {
        console.error('[rill:reconciler] component render error', err);
        return null;
      }
    },
    {
      // Mark: this is a sandbox wrapper (for transformGuestElement to decide children handling)
      __rillSandboxWrapper: true as const,
      displayName: displayName,
    }
  ) as SandboxWrapper;

  // Copy static properties from originalType
  try {
    Object.assign(wrapped, originalType);
  } catch {
    // ignore
  }

  componentWrappers.set(key, wrapped);
  return wrapped;
}

// ============================================
// Guest Element Transformation
// ============================================

/**
 * Transform Guest element to use Host's Symbol registry
 *
 * When Guest (JSC sandbox) creates React elements, it uses its own Symbol registry.
 * Host (Hermes) has a different Symbol registry, so Symbol.for('react.element')
 * returns different Symbols in each engine. Additionally, JSI doesn't preserve
 * Symbols across the engine boundary at all - they become undefined.
 *
 * We use string markers (__rillTypeMarker, __rillFragmentType) that survive
 * JSI serialization to identify React elements from the Guest.
 *
 * This function recursively transforms Guest elements to use Host Symbols.
 */
export function transformGuestElement(
  element: GuestElement,
  autoKey?: string
): ReactElement | null {
  if (element === null || element === undefined) {
    return null;
  }

  // Handle primitive values (text nodes)
  if (typeof element !== 'object') {
    return element as unknown as ReactElement;
  }

  // Handle arrays (children)
  if (Array.isArray(element)) {
    return element.map((child, idx) =>
      transformGuestElement(child, String(idx))
    ) as unknown as ReactElement;
  }

  // At this point, element must be a GuestReactElement
  if (!isGuestReactElement(element)) {
    // Unexpected type, return as-is
    return element as unknown as ReactElement;
  }

  const el = element as GuestElementRuntime;
  try {
    const marker = el.__rillTypeMarker;
    const typeVal = el.type;
    const typeType = typeof typeVal;
    const propsKeys = el.props ? Object.keys(el.props) : null;
    const globalState = globalThis as Record<string, unknown>;
    if (globalState.__RILL_RECONCILER_DEBUG__) {
      // Only log first 20 transforms to avoid spam
      const counter = (globalState.__TRANSFORM_LOG_COUNT as number | undefined) || 0;
      if (counter < 20) {
        globalState.__TRANSFORM_LOG_COUNT = counter + 1;
        console.log(
          `[rill:reconciler] transform element | marker=${String(marker)} | typeType=${typeType} | type=${String(typeVal)} | propsKeys=${propsKeys ? propsKeys.join(',') : 'null'}`
        );
      }
    }
  } catch {
    // ignore logging failures
  }

  // Check if this is a Rill Guest element using the string marker
  // This marker survives JSI serialization while Symbols don't
  const isRillElement = el.__rillTypeMarker === RILL_ELEMENT_MARKER;

  // Also check for $$typeof Symbol (works when not crossing JSI, e.g., NoSandbox provider)
  const hasSymbolType = typeof el.$$typeof === 'symbol';

  // If already Host ReactElement (has $$typeof symbol but no Rill marker), return directly,
  // avoiding double transform/wrap, especially avoiding passing Host ReactElement back to sandbox.
  if (hasSymbolType && !isRillElement) {
    if (
      autoKey !== undefined &&
      (el.key === undefined || el.key === null) &&
      typeof React.cloneElement === 'function'
    ) {
      try {
        return React.cloneElement(element as ReactElement, { key: autoKey });
      } catch {
        // ignore
      }
    }
    return element as ReactElement;
  }

  // Heuristic: some elements lose both marker and $$typeof across JSI bridge.
  // If shape still looks like a React element (has type/props), treat it as one.
  const looksLikeElement =
    !isRillElement && !hasSymbolType && Object.hasOwn(el, 'type') && Object.hasOwn(el, 'props');

  if (looksLikeElement) {
    console.error(
      `[rill:reconciler] missing markers, treating as element | typeType=${typeof el.type} | propsKeys=${
        el.props ? Object.keys(el.props as Record<string, unknown>).join(',') : 'null'
      }`
    );
  }

  if (!isRillElement && !hasSymbolType && !looksLikeElement) {
    // Not a React element, return as-is
    return element as ReactElement;
  }

  // Determine if this is a Fragment
  const isFragment =
    el.__rillFragmentType === RILL_FRAGMENT_MARKER ||
    (typeof el.type === 'symbol' && (el.type as symbol).description === 'react.fragment');

  // Transform the element type
  let transformedType: unknown = el.type;
  if (isFragment) {
    transformedType = REACT_FRAGMENT_TYPE;
  } else if (isComponentTypeRef(transformedType)) {
    const ref = transformedType;
    const resolved = componentTypeFnById.get(ref.__rillComponentId);
    if (resolved) {
      transformedType = getOrCreateWrappedComponent(resolved, ref.displayName);
    } else {
      // If registry misses, fall back to Fragment to preserve children
      transformedType = REACT_FRAGMENT_TYPE;
    }
  } else if (typeof transformedType === 'function') {
    transformedType = getOrCreateWrappedComponent(transformedType);
  } else if (typeof transformedType === 'object' && transformedType !== null) {
    // Some guests may accidentally pass a React element (object) as type
    // Try to unwrap its `type` field; otherwise bail out to avoid invalid type errors
    const typeObj = transformedType as Record<string, unknown>;
    const nestedType = typeObj.type;
    const renderType = typeObj.render;
    const defaultType = typeObj.default;
    const typeKeys = Object.keys(typeObj);
    const ownKeys = Reflect.ownKeys(typeObj).map((k) => String(k));
    const symbolType = typeObj.$$typeof;
    const displayName = typeObj.displayName;
    const protoName = Object.getPrototypeOf(typeObj)?.constructor?.name ?? 'null';
    const typeTag = Object.prototype.toString.call(typeObj);
    const isCallable =
      typeof typeObj === 'function' ||
      typeObj instanceof Function ||
      typeof typeObj.call === 'function';
    console.error(
      `[rill:reconciler] type is object, attempting unwrap | keys=${typeKeys.join(
        ','
      )} | ownKeys=${ownKeys.join(',')} | $$typeof=${String(symbolType)} | nestedType=${String(
        nestedType
      )} | renderType=${String(renderType)} | defaultType=${String(defaultType)} | displayName=${String(
        displayName
      )} | proto=${protoName} | tag=${typeTag} | callable=${isCallable}`
    );
    const panelId =
      typeof displayName === 'string' && displayName.toLowerCase().includes('panel.')
        ? displayName.toLowerCase().includes('left')
          ? 'left'
          : displayName.toLowerCase().includes('right')
            ? 'right'
            : null
        : null;
    if (panelId) {
      // Heuristic fallback: rebuild Panel.{Left,Right} when markers lost across bridge
      const fallbackPanel: React.FC<{ children?: React.ReactNode }> = ({ children }) =>
        React.createElement('PanelMarker', { panelId, children });
      if (typeof displayName === 'string') {
        fallbackPanel.displayName = displayName;
      }
      transformedType = fallbackPanel;
    }

    const candidates = [nestedType, renderType, defaultType];
    const chosen = candidates.find(
      (candidate) => typeof candidate === 'string' || typeof candidate === 'function'
    );
    if (!chosen) {
      if (!panelId) {
        // Fall back to Fragment to preserve children even when type metadata is lost
        transformedType = REACT_FRAGMENT_TYPE;
      }
    } else {
      transformedType = typeof chosen === 'function' ? getOrCreateWrappedComponent(chosen) : chosen;
    }
  }

  // Transform children recursively
  const props = el.props;
  let transformedProps = props;
  const isSandboxWrapper =
    typeof transformedType === 'function' &&
    '__rillSandboxWrapper' in transformedType &&
    transformedType.__rillSandboxWrapper === true;
  const shouldPreserveChildrenForSandboxWrapper =
    props && props.children !== undefined && isSandboxWrapper;

  // Serialize props for Guest components (sandbox wrappers)
  // TypeRules automatically handle function → { __type: 'function', __fnId }
  if (props && isSandboxWrapper) {
    const { children, ...restProps } = props;
    const serialized = serializeProps(restProps);
    if (children !== undefined) {
      serialized.children = children as SerializedValue;
    }
    transformedProps = serialized;
  }

  if (props && props.children !== undefined && !shouldPreserveChildrenForSandboxWrapper) {
    const transformedChildren = Array.isArray(props.children)
      ? props.children.map((child, idx) =>
          transformGuestElement(child as GuestElement, String(idx))
        )
      : transformGuestElement(props.children as GuestElement);
    transformedProps = { ...transformedProps, children: transformedChildren };
  }

  const key =
    el.key !== undefined && el.key !== null
      ? (el.key as React.Key)
      : autoKey !== undefined
        ? (autoKey as React.Key)
        : null;

  // Use React.createElement to ensure correct Symbol and structure
  const config = {
    ...transformedProps,
    key,
    ref: el.ref,
  };

  // If transformedType is fragment symbol, use React.Fragment
  if (transformedType === REACT_FRAGMENT_TYPE) {
    // Fragment only accepts key + children. Strip other props to avoid React warnings
    const fragmentKey = key as React.Key | null | undefined;
    const fragmentChildren = transformedProps?.children as React.ReactNode;
    return React.createElement(React.Fragment, { key: fragmentKey }, fragmentChildren);
  }

  return React.createElement(transformedType as React.ElementType, config);
}
