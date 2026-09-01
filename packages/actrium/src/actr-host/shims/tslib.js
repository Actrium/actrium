'use strict';

// tslib It's... CJS/ESM Interoperating shim.
//
// Why do you need this document:
//   tslib It's... CJS Only name at entry helper(__extends, __awaiter I am not a writer.
//   No, I don't. default exportBut part of it. ESM Dependency
//   The long term Metro I'll pack it. `tslib.default.__awaiter` The way to visit,
//   Result `Cannot read property '__awaiter' of undefined`.
//
// Solutions:
//   Load real tslib CJS module, then put module.exports.default
//   And it points to the same object, so whatever. named import Or is it? default import I'm not sure if I can work.

const tslib = require('tslib');

module.exports = tslib;
module.exports.default = tslib;
