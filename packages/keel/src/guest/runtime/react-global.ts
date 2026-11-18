/**
 * React Global Setup (Guest Runtime)
 *
 * Sets up React and JSX runtimes on globalThis for:
 * - Guest bundles that reference globals (externalized builds)
 * - Engine.require('react' | 'react/jsx-runtime') resolution
 *
 * NOTE: This module must run before keel/guest is evaluated, because
 * keel/guest defines some exports (e.g., ErrorBoundary) at module init time.
 */

import * as React from 'react';
import * as ReactJSXRuntime from 'react/jsx-runtime';
import type { TextInputProps } from '../../sdk';
import type { ReviewedUnknown } from '../../shared';
import type { TextInputEdit, TextInputSyncProps } from '../../shared/text-input';

const GuestTextInput = React.forwardRef<unknown, Omit<TextInputProps, 'ref'>>((props, ref) => {
  const { onChange, onChangeText } = props;
  const callbacks = React.useRef({ onChange, onChangeText });
  React.useLayoutEffect(() => {
    callbacks.current = { onChange, onChangeText };
  }, [onChange, onChangeText]);

  const [eventCount, setEventCount] = React.useState(0);
  const lastEdit = React.useRef(0);
  const onEdit = React.useCallback((edit: TextInputEdit) => {
    if (!Number.isSafeInteger(edit.eventCount) || edit.eventCount <= lastEdit.current) return;
    lastEdit.current = edit.eventCount;
    callbacks.current.onChange?.({ nativeEvent: edit });
    callbacks.current.onChangeText?.(edit.text);
    // Send an acknowledgment even when the controlled value did not change.
    setEventCount(edit.eventCount);
  }, []);
  const sync: TextInputSyncProps =
    typeof props.value === 'string' ? { __keelTextInput: { eventCount, onEdit } } : {};
  return React.createElement('TextInput', { ...props, ...sync, ref });
});

// Keep the public virtual tag and add synchronization inside the Guest runtime.
function elementType(type: React.ElementType | string): React.ElementType {
  return type === 'TextInput' ? GuestTextInput : (type as React.ElementType);
}

const guestJSXRuntime = {
  ...ReactJSXRuntime,
  jsx: (type: React.ElementType | string, props: ReviewedUnknown, key?: React.Key) =>
    ReactJSXRuntime.jsx(elementType(type), props, key),
  jsxs: (type: React.ElementType | string, props: ReviewedUnknown, key?: React.Key) =>
    ReactJSXRuntime.jsxs(elementType(type), props, key),
};

// Mark React as loaded (using real React, not shim)
(globalThis as Record<string, unknown>).__REACT_SHIM__ = false;
(globalThis as Record<string, unknown>).__REACT_REAL__ = true;

// Export React to globalThis
(globalThis as Record<string, unknown>).React = {
  ...React,
  createElement: (
    type: React.ElementType,
    props?: React.Attributes | null,
    ...children: React.ReactNode[]
  ) => React.createElement(elementType(type), props, ...children),
};
(globalThis as Record<string, unknown>).ReactJSXRuntime = guestJSXRuntime;
(globalThis as Record<string, unknown>).ReactJSXDevRuntime = guestJSXRuntime;
