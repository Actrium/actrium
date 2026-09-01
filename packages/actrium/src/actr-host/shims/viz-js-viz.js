'use strict';

// Metro bundling shim for @viz-js/viz.
//
// lib/backend.js uses `import.meta.url`, which Hermes cannot parse. The Metro
// resolver redirects @viz-js/viz here so the incompatible module never enters
// the bundle.
//
// dot.ts loads this shim; renderString throws and the caller converts the
// failure into the diagram error UI.

async function instance() {
  return {
    renderString() {
      throw new Error('Graphviz (dot) WASM is unavailable on React Native/Hermes');
    },
  };
}

module.exports = {
  instance,
};
