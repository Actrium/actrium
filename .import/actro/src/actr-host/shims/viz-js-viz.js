'use strict';

// @viz-js/viz 的 Metro 打包 shim。
//
// 为什么需要这个文件：
//   @viz-js/viz 的真实源码（lib/backend.js）使用了 `import.meta.url`，
//   这是 ESM 语法，Hermes 引擎无法解析，会在 bundle 加载阶段直接报语法错误。
//   通过 metro.config.js 的 resolveRequest 将 `@viz-js/viz` 重定向到本文件，
//   使真实模块根本不会被打进 bundle，从而绕过语法问题。
//
// 运行时行为：
//   dot.ts 中 `await import('@viz-js/viz')` 实际加载的是本 shim。
//   instance() 返回的 renderString() 会抛错，被上层转成 diagram error UI。

async function instance() {
  return {
    renderString() {
      throw new Error('Graphviz (dot) WASM 在 React Native/Hermes 上不可用');
    },
  };
}

module.exports = {
  instance,
};
