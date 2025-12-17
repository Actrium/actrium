/**
 * Shared shims for Guest sandbox runtime
 *
 * These shims provide minimal React/JSX runtime compatibility in the sandbox.
 * They are injected into the Guest context via eval(), so React runs entirely
 * within the Guest - no cross-engine serialization of React elements needed.
 *
 * Used by all providers: JSC Sandbox, QuickJS Worker, VM, NoSandbox
 */

/**
 * React shim code - provides minimal React API for Guest
 *
 * Key features:
 * - createElement: Creates React element objects with $$typeof Symbol
 * - Fragment: Symbol for React fragments
 * - Hooks: Stub implementations (real state management happens in reconciler)
 *
 * IMPORTANT: JSI doesn't preserve Symbols across the JS engine boundary.
 * We use __rillTypeMarker (string) as a backup to identify React elements
 * when they cross from Guest (JSC) to Host (Hermes).
 */
export const REACT_SHIM = `
(function() {
  var Fragment = Symbol.for('react.fragment');
  // String markers for JSI serialization (Symbols don't survive JSI boundary)
  var RILL_ELEMENT_MARKER = '__rill_react_element__';
  var RILL_FRAGMENT_MARKER = '__rill_react_fragment__';

  function createElement(type, props) {
    var children = [];
    for (var i = 2; i < arguments.length; i++) {
      var child = arguments[i];
      if (child != null) children.push(child);
    }

    // Determine the type marker for Fragment
    var typeMarker = (type === Fragment || type === RILL_FRAGMENT_MARKER) ? RILL_FRAGMENT_MARKER : null;

    var element = {
      $$typeof: Symbol.for('react.element'),
      // String marker that survives JSI serialization
      __rillTypeMarker: RILL_ELEMENT_MARKER,
      type: type,
      // Store original type as string for Fragment (since Symbol doesn't survive JSI)
      __rillFragmentType: typeMarker,
      key: props && props.key != null ? String(props.key) : null,
      ref: props && props.ref !== undefined ? props.ref : null,
      props: (function(p, kids) {
        var cp = {};
        if (p) {
          for (var k in p) {
            if (k !== 'key' && k !== 'ref') cp[k] = p[k];
          }
        }
        if (kids.length === 1) cp.children = kids[0];
        else if (kids.length > 1) cp.children = kids;
        return cp;
      })(props, children),
      _owner: null,
      _store: {}
    };

    return element;
  }

  var React = {
    createElement: createElement,
    Fragment: Fragment,
    // Minimal hooks stubs - real state is managed by reconciler
    useState: function(init) {
      return [typeof init === 'function' ? init() : init, function() {}];
    },
    useEffect: function() {},
    useLayoutEffect: function() {},
    useCallback: function(fn) { return fn; },
    useMemo: function(fn) { return fn(); },
    useRef: function(init) { return { current: init }; },
    useContext: function(ctx) { return ctx && ctx._currentValue; },
    useReducer: function(reducer, init) { return [init, function() {}]; },
    useImperativeHandle: function() {},
    useDebugValue: function() {},
    // Higher-order components
    memo: function(component) { return component; },
    forwardRef: function(component) { return component; },
    createContext: function(defaultValue) {
      return {
        Provider: 'Provider',
        Consumer: 'Consumer',
        _currentValue: defaultValue
      };
    },
    // React 18+ compatibility
    startTransition: function(fn) { fn(); },
    useTransition: function() { return [false, function(fn) { fn(); }]; },
    useDeferredValue: function(value) { return value; },
    useId: function() { return 'id-' + Math.random().toString(36).substr(2, 9); },
    useSyncExternalStore: function(subscribe, getSnapshot) { return getSnapshot(); },
    // Utilities
    Children: {
      map: function(children, fn) {
        if (children == null) return [];
        if (Array.isArray(children)) return children.map(fn);
        return [fn(children, 0)];
      },
      forEach: function(children, fn) {
        if (children == null) return;
        if (Array.isArray(children)) children.forEach(fn);
        else fn(children, 0);
      },
      count: function(children) {
        if (children == null) return 0;
        if (Array.isArray(children)) return children.length;
        return 1;
      },
      only: function(children) {
        if (Array.isArray(children) && children.length === 1) return children[0];
        if (!Array.isArray(children) && children != null) return children;
        throw new Error('React.Children.only expected to receive a single React element child.');
      },
      toArray: function(children) {
        if (children == null) return [];
        if (Array.isArray(children)) return children.flat();
        return [children];
      }
    },
    isValidElement: function(object) {
      return (
        typeof object === 'object' &&
        object !== null &&
        object.$$typeof === Symbol.for('react.element')
      );
    },
    cloneElement: function(element, props) {
      var newProps = Object.assign({}, element.props, props);
      var children = [];
      for (var i = 2; i < arguments.length; i++) {
        children.push(arguments[i]);
      }
      if (children.length > 0) {
        newProps.children = children.length === 1 ? children[0] : children;
      }
      return {
        $$typeof: Symbol.for('react.element'),
        type: element.type,
        key: props && props.key !== undefined ? props.key : element.key,
        ref: props && props.ref !== undefined ? props.ref : element.ref,
        props: newProps,
        _owner: element._owner,
        _store: {}
      };
    },
    version: '18.0.0-rill-shim'
  };

  globalThis.React = React;
  globalThis.__REACT_SHIM__ = true;

  // ReactNative shim - provides component name strings
  // Bundler maps react-native imports to these string names
  // The reconciler converts them to actual RN components on host side
  var ReactNative = {
    View: 'View',
    Text: 'Text',
    Image: 'Image',
    ScrollView: 'ScrollView',
    TouchableOpacity: 'TouchableOpacity',
    Button: 'Button',
    ActivityIndicator: 'ActivityIndicator',
    FlatList: 'FlatList',
    TextInput: 'TextInput',
    Switch: 'Switch',
    Pressable: 'Pressable',
    SafeAreaView: 'SafeAreaView',
    StatusBar: 'StatusBar',
    Modal: 'Modal',
    Alert: { alert: function() { console.warn('[rill] Alert.alert called in sandbox'); } },
    StyleSheet: {
      create: function(styles) { return styles; },
      flatten: function(style) {
        if (!style) return {};
        if (Array.isArray(style)) {
          var result = {};
          for (var i = 0; i < style.length; i++) {
            if (style[i]) Object.assign(result, style[i]);
          }
          return result;
        }
        return style;
      }
    },
    Dimensions: {
      get: function() { return { width: 375, height: 812, scale: 3, fontScale: 1 }; }
    },
    Platform: {
      OS: 'ios',
      select: function(obj) { return obj.ios || obj.default; }
    }
  };
  globalThis.ReactNative = ReactNative;
})();
`;

/**
 * ReactJSXRuntime shim - provides jsx/jsxs functions for compiled JSX
 *
 * IMPORTANT: JSI doesn't preserve Symbols across the JS engine boundary.
 * We use __rillTypeMarker (string) as a backup to identify React elements.
 */
export const JSX_RUNTIME_SHIM = `
(function() {
  var Fragment = Symbol.for('react.fragment');
  // String markers for JSI serialization (Symbols don't survive JSI boundary)
  var RILL_ELEMENT_MARKER = '__rill_react_element__';
  var RILL_FRAGMENT_MARKER = '__rill_react_fragment__';

  function jsx(type, props, key) {
    var children = props && props.children;
    var newProps = {};

    for (var k in props) {
      if (k !== 'key' && k !== 'ref' && k !== 'children') {
        newProps[k] = props[k];
      }
    }

    if (children !== undefined) {
      newProps.children = children;
    }

    // Determine the type marker for Fragment
    var typeMarker = (type === Fragment || type === RILL_FRAGMENT_MARKER) ? RILL_FRAGMENT_MARKER : null;

    return {
      $$typeof: Symbol.for('react.element'),
      // String marker that survives JSI serialization
      __rillTypeMarker: RILL_ELEMENT_MARKER,
      type: type,
      // Store original type as string for Fragment (since Symbol doesn't survive JSI)
      __rillFragmentType: typeMarker,
      key: key != null ? String(key) : (props && props.key != null ? String(props.key) : null),
      ref: props && props.ref !== undefined ? props.ref : null,
      props: newProps,
      _owner: null,
      _store: {}
    };
  }

  var ReactJSXRuntime = {
    jsx: jsx,
    jsxs: jsx,  // jsxs is same as jsx for our purposes
    Fragment: Fragment
  };

  globalThis.ReactJSXRuntime = ReactJSXRuntime;
  globalThis.__JSX_RUNTIME_SHIM__ = true;
})();
`;

/**
 * Console shim - prevents errors when console methods are called
 * Actual logging is handled by __console_log etc. set by engine
 * Note: Always override console methods to ensure they use our __console_* functions
 */
export const CONSOLE_SHIM = `
(function() {
  // Always set up console with our __console_* functions
  // JSC sandbox may have a console object but its methods might not work
  if (typeof globalThis.console === 'undefined') {
    globalThis.console = {};
  }

  // Override all logging methods with our implementations
  globalThis.console.log = typeof __console_log === 'function' ? __console_log : function() {};
  globalThis.console.warn = typeof __console_warn === 'function' ? __console_warn : function() {};
  globalThis.console.error = typeof __console_error === 'function' ? __console_error : function() {};
  globalThis.console.debug = typeof __console_debug === 'function' ? __console_debug : function() {};
  globalThis.console.info = typeof __console_info === 'function' ? __console_info : function() {};

  // Stub out other console methods
  globalThis.console.trace = globalThis.console.trace || function() {};
  globalThis.console.assert = globalThis.console.assert || function() {};
  globalThis.console.clear = globalThis.console.clear || function() {};
  globalThis.console.count = globalThis.console.count || function() {};
  globalThis.console.countReset = globalThis.console.countReset || function() {};
  globalThis.console.group = globalThis.console.group || function() {};
  globalThis.console.groupCollapsed = globalThis.console.groupCollapsed || function() {};
  globalThis.console.groupEnd = globalThis.console.groupEnd || function() {};
  globalThis.console.table = globalThis.console.table || function() {};
  globalThis.console.time = globalThis.console.time || function() {};
  globalThis.console.timeEnd = globalThis.console.timeEnd || function() {};
  globalThis.console.timeLog = globalThis.console.timeLog || function() {};
  globalThis.console.dir = globalThis.console.dir || function() {};
  globalThis.console.dirxml = globalThis.console.dirxml || function() {};
})();
`;

/**
 * Combined shims - all shims needed for Guest runtime
 * Order matters: console first, then React, then JSX runtime
 */
export const ALL_SHIMS = `
${CONSOLE_SHIM}
${REACT_SHIM}
${JSX_RUNTIME_SHIM}
`;

/**
 * Check if React shim is already injected
 */
export const IS_SHIM_INJECTED = `(typeof globalThis.__REACT_SHIM__ === 'boolean' && globalThis.__REACT_SHIM__)`;
