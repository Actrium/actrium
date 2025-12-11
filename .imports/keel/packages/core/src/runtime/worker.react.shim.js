// Richer React/JSXRuntime shim for worker QuickJS
// This is not full React; it only provides enough surface for compiled JSX to run in sandbox.

(function(){
  var Fragment = Symbol.for('react.fragment');

  function createElement(type, props) {
    var children = [];
    for (var i = 2; i < arguments.length; i++) children.push(arguments[i]);
    return {
      $$typeof: Symbol.for('react.element'),
      type: type,
      key: props && props.key != null ? String(props.key) : null,
      ref: props && props.ref !== undefined ? props.ref : null,
      props: (function(p, kids){
        var cp = {};
        if (p) for (var k in p) if (k !== 'key' && k !== 'ref') cp[k] = p[k];
        if (kids.length === 1) cp.children = kids[0];
        else if (kids.length > 1) cp.children = kids;
        return cp;
      })(props, children),
      _owner: null,
    };
  }

  var React = {
    createElement: createElement,
    Fragment: Fragment,
    // minimal hooks stubs to avoid ReferenceErrors; these are NOOPs in sandbox
    useState: function(init){ return [init, function(){}]; },
    useEffect: function(){},
    useMemo: function(fn){ return fn && fn(); },
    useRef: function(v){ return { current: v }; },
  };

  var ReactJSXRuntime = {
    Fragment: Fragment,
    jsx: function(type, props, key){
      return React.createElement(type, props, ...(props && props.children != null ? [props.children] : []));
    },
    jsxs: function(type, props, key){
      return React.createElement(type, props, ...(Array.isArray(props && props.children) ? props.children : [props && props.children].filter(Boolean)));
    },
  };

  globalThis.React = React;
  globalThis.ReactJSXRuntime = ReactJSXRuntime;
})();
