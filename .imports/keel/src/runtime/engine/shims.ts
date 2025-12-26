/**
 * React/JSX Shims for Guest Sandbox
 *
 * Complete React implementation that runs in the sandboxed environment.
 * Includes hooks system, JSX runtime, and React Native component registry.
 */

// React/JSX shims for Guest sandbox
export const ALL_SHIMS = `
// Mark shims as injected
globalThis.__REACT_SHIM__ = true;

// Hook state management - implements React hooks system
globalThis.__rillHooks = {
  // useState storage
  states: [],
  // useRef storage
  refs: [],
  // useMemo storage: { deps, value }
  memos: [],
  // useEffect storage: { effect, deps, cleanup }
  effects: [],
  // Pending effects to run after render
  pendingEffects: [],
  // useId storage
  ids: [],
  // useId counter
  idCounter: 0,
  // Current hook index (reset before each render)
  index: 0,
  // Prevent recursive re-renders
  isRendering: false,
  // Context registry: Map<ContextType, value>
  contexts: new Map(),
  // Root element for re-renders
  rootElement: null,
  sendToHost: null
};

// Shallow compare two arrays (for deps comparison)
globalThis.__rillDepsEqual = function(prevDeps, nextDeps) {
  if (prevDeps === nextDeps) return true;
  if (prevDeps === null || nextDeps === null) return false;
  if (prevDeps === undefined || nextDeps === undefined) return prevDeps === nextDeps;
  if (prevDeps.length !== nextDeps.length) return false;
  for (let i = 0; i < prevDeps.length; i++) {
    if (!Object.is(prevDeps[i], nextDeps[i])) return false;
  }
  return true;
};

// Run pending effects after render
globalThis.__rillFlushEffects = function() {
  const hooks = globalThis.__rillHooks;
  const pending = hooks.pendingEffects;
  hooks.pendingEffects = [];

  for (const { index, effect, prevCleanup } of pending) {
    // Run previous cleanup if exists
    if (typeof prevCleanup === 'function') {
      try { prevCleanup(); } catch (e) { console.error('[rill] Effect cleanup error:', e); }
    }
    // Run effect and store cleanup
    try {
      const cleanup = effect();
      if (hooks.effects[index]) {
        hooks.effects[index].cleanup = cleanup;
      }
    } catch (e) {
      console.error('[rill] Effect error:', e);
    }
  }
};

// Wrapper for render to track root and enable re-renders
globalThis.__rillWrapRender = function(originalRender) {
  return function(element, sendToHost) {
    const hooks = globalThis.__rillHooks;
    hooks.index = 0;
    hooks.rootElement = element;
    hooks.sendToHost = sendToHost;
    const result = originalRender(element, sendToHost);
    // Flush effects after render (async to not block render)
    if (hooks.pendingEffects.length > 0) {
      Promise.resolve().then(globalThis.__rillFlushEffects);
    }
    return result;
  };
};

// Schedule re-render helper
globalThis.__rillScheduleRender = function() {
  const hooks = globalThis.__rillHooks;
  if (hooks.isRendering) return;

  hooks.isRendering = true;
  hooks.index = 0;
  try {
    if (typeof globalThis.__rill_schedule_render === 'function') {
      globalThis.__rill_schedule_render();
    } else {
      try {
        const reconciler = require('rill/reconciler');
        if (reconciler.scheduleRender) {
          reconciler.scheduleRender();
        } else if (hooks.rootElement && hooks.sendToHost) {
          reconciler.render(hooks.rootElement, hooks.sendToHost);
        }
      } catch (e) {
        console.error('[rill:shim] scheduleRender failed:', e);
      }
    }
  } finally {
    hooks.isRendering = false;
  }
};

// Minimal React shim for Guest
globalThis.React = {
  createElement: function(type, props, ...children) {
    const element = {
      __rillTypeMarker: '__rill_react_element__',
      type: type,
      props: {
        ...(props || {}),
        children: children.length === 1 ? children[0] : (children.length > 1 ? children : undefined)
      }
    };
    return element;
  },
  Fragment: '__rill_fragment__',

  // ============ useState ============
  useState: function(initialValue) {
    const hooks = globalThis.__rillHooks;
    const currentIndex = hooks.index++;

    // Initialize on first render
    if (currentIndex >= hooks.states.length) {
      const value = typeof initialValue === 'function' ? initialValue() : initialValue;
      hooks.states.push(value);
    }

    const setState = (newValue) => {
      const idx = currentIndex;
      const prevValue = hooks.states[idx];
      const nextValue = typeof newValue === 'function' ? newValue(prevValue) : newValue;

      if (!Object.is(nextValue, prevValue)) {
        hooks.states[idx] = nextValue;
        globalThis.__rillScheduleRender();
      }
    };

    return [hooks.states[currentIndex], setState];
  },

  // ============ useRef ============
  useRef: function(initialValue) {
    const hooks = globalThis.__rillHooks;
    const currentIndex = hooks.index++;

    // Initialize on first render
    if (currentIndex >= hooks.refs.length) {
      hooks.refs.push({ current: initialValue });
    }

    return hooks.refs[currentIndex];
  },

  // ============ useMemo ============
  useMemo: function(factory, deps) {
    const hooks = globalThis.__rillHooks;
    const currentIndex = hooks.index++;

    const prevMemo = hooks.memos[currentIndex];

    // Check if deps changed
    if (prevMemo !== undefined && globalThis.__rillDepsEqual(prevMemo.deps, deps)) {
      return prevMemo.value;
    }

    // Compute new value
    const value = factory();
    hooks.memos[currentIndex] = { deps: deps, value: value };
    return value;
  },

  // ============ useCallback ============
  useCallback: function(callback, deps) {
    return globalThis.React.useMemo(function() { return callback; }, deps);
  },

  // ============ useReducer ============
  useReducer: function(reducer, initialArg, init) {
    const initialState = init ? init(initialArg) : initialArg;
    const result = globalThis.React.useState(initialState);
    const state = result[0];
    const setState = result[1];

    const dispatch = globalThis.React.useCallback(function(action) {
      setState(function(prevState) {
        return reducer(prevState, action);
      });
    }, [reducer]);

    return [state, dispatch];
  },

  // ============ useId ============
  useId: function() {
    const hooks = globalThis.__rillHooks;
    const currentIndex = hooks.index++;

    // Initialize on first render
    if (hooks.ids === undefined) {
      hooks.ids = [];
    }
    if (currentIndex >= hooks.ids.length) {
      // Generate unique ID: prefix + counter
      const id = 'rill:r' + (++hooks.idCounter);
      hooks.ids.push(id);
    }

    return hooks.ids[currentIndex];
  },

  // ============ useEffect ============
  useEffect: function(effect, deps) {
    const hooks = globalThis.__rillHooks;
    const currentIndex = hooks.index++;

    const prevEffect = hooks.effects[currentIndex];

    // Check if deps changed (undefined deps = always run)
    const shouldRun = prevEffect === undefined ||
                      deps === undefined ||
                      !globalThis.__rillDepsEqual(prevEffect.deps, deps);

    if (shouldRun) {
      // Store effect info
      hooks.effects[currentIndex] = { effect: effect, deps: deps, cleanup: undefined };
      // Queue effect to run after render
      hooks.pendingEffects.push({
        index: currentIndex,
        effect: effect,
        prevCleanup: prevEffect ? prevEffect.cleanup : undefined
      });
    }
  },

  // ============ useContext ============
  useContext: function(Context) {
    const hooks = globalThis.__rillHooks;
    // Look up context value from registry
    if (hooks.contexts.has(Context)) {
      return hooks.contexts.get(Context);
    }
    // Return default value if not provided
    return Context && Context._currentValue !== undefined ? Context._currentValue : undefined;
  },

  // ============ createContext ============
  createContext: function(defaultValue) {
    const context = {
      _currentValue: defaultValue,
      Provider: function(props) {
        // Register context value
        globalThis.__rillHooks.contexts.set(context, props.value);
        return props.children;
      },
      Consumer: function(props) {
        const value = globalThis.React.useContext(context);
        return props.children(value);
      }
    };
    return context;
  }
};

// JSX Runtime shim
globalThis.ReactJSXRuntime = {
  jsx: globalThis.React.createElement,
  jsxs: globalThis.React.createElement,
  Fragment: globalThis.React.Fragment
};
globalThis.ReactJSXDevRuntime = globalThis.ReactJSXRuntime;

// ReactNative shim (component name registry)
globalThis.ReactNative = {
  View: 'View',
  Text: 'Text',
  Image: 'Image',
  ScrollView: 'ScrollView',
  TouchableOpacity: 'TouchableOpacity',
  Button: 'Button',
  ActivityIndicator: 'ActivityIndicator',
  FlatList: 'FlatList',
  TextInput: 'TextInput',
  Switch: 'Switch'
};
`;

/**
 * DevTools Guest shim
 * Provides error capturing and reporting to host DevTools
 */
export const DEVTOOLS_SHIM = `
if (typeof globalThis.__sendEventToHost === 'function') {
  globalThis.__rill_devtools_guest = {
    captureError: function(error, context) {
      globalThis.__sendEventToHost('devtools:error', {
        message: error?.message || String(error),
        stack: error?.stack,
        context: context
      });
    }
  };
}
`;
