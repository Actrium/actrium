/* Ensure console.assert exists as early as possible in the RN bundle lifecycle */
(function(){
  try {
    var root = (typeof globalThis !== 'undefined' ? globalThis : (typeof global !== 'undefined' ? global : (typeof window !== 'undefined' ? window : this)));
    var c = root.console;
    if (!c) {
      c = {};
      try { root.console = c; } catch(_){}
    }
    if (typeof c.assert !== 'function') {
      c.assert = function(expression){
        if (!expression) {
          var args = Array.prototype.slice.call(arguments, 1);
          try { if (c && typeof c.error === 'function') c.error.apply(c, ['Assertion failed:'].concat(args)); } catch(_){}
        }
      };
    }
    // Guard future assignments to global console to always include assert
    var desc = Object.getOwnPropertyDescriptor(root, 'console');
    if (!desc || desc.configurable) {
      var currentConsole = root.console || c;
      Object.defineProperty(root, 'console', {
        configurable: true,
        enumerable: true,
        get: function(){ return currentConsole; },
        set: function(v){
          currentConsole = v || {};
          if (typeof currentConsole.assert !== 'function') {
            currentConsole.assert = c.assert;
          }
        }
      });
      // enforce on existing console
      root.console = currentConsole;
    }

    // RN/Hermes compatibility: some libraries (e.g. vega stack) expect structuredClone.
    if (typeof root.structuredClone !== 'function') {
      root.structuredClone = function(value){
        try {
          return JSON.parse(JSON.stringify(value));
        } catch (_) {
          return value;
        }
      };
    }

    try {
      // Keep ultra-early log minimal; logger may not be available yet.
      root.console && root.console.log && root.console.log('[ensure-console-assert] loaded before main');
    } catch (_) {}

  } catch(_){ }
})();
