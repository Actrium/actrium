'use strict';

function unavailable(name) {
  throw new Error(
    `${name} is a Web/WASM engine and is not bundled in React Native. ` +
      'Register the matching Supramark native RN adapter instead.'
  );
}

function convert() {
  unavailable('Supramark diagram');
}

function render() {
  unavailable('Supramark diagram');
}

function renderSvg() {
  unavailable('Supramark diagram');
}

function init() {
  return undefined;
}

function version() {
  return 'react-native-shim';
}

function setup() {
  return function dispose() {};
}

function installGraphvizBridge() {
  return function dispose() {};
}

function hasGraphvizBridge() {
  return false;
}

const Graphviz = {
  async load() {
    return {
      layout() {
        unavailable('@actrium/graphviz-anywhere-web');
      },
    };
  },
};

async function createGraphviz() {
  return Graphviz.load();
}

module.exports = {
  default: init,
  init,
  convert,
  convertWithId: convert,
  convert_with_id: convert,
  render,
  renderSvg,
  version,
  setup,
  installGraphvizBridge,
  hasGraphvizBridge,
  Graphviz,
  createGraphviz,
};
