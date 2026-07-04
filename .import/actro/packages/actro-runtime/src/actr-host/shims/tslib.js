'use strict';

// tslib 的 CJS/ESM 互操作 shim。
//
// 为什么需要这个文件：
//   tslib 的 CJS 入口只导出具名 helper（__extends、__awaiter 等），
//   没有 default export。但部分依赖包（如 supramark 内的 ESM 模块）
//   经 Metro 打包后会以 `tslib.default.__awaiter` 的方式访问，
//   导致 `Cannot read property '__awaiter' of undefined`。
//
// 解决方式：
//   加载真实的 tslib CJS 模块，然后把 module.exports.default
//   也指向同一个对象，这样无论 named import 还是 default import 都能正常工作。

const tslib = require('tslib');

module.exports = tslib;
module.exports.default = tslib;
