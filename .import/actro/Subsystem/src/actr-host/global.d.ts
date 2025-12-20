// NOTE: RN runtime may not provide Node.js `process` at runtime, but some shared code
// (e.g. rill) uses `process.versions` for environment detection.
// Keep the type broad and compatible with @types/node.

declare const process: NodeJS.Process | undefined;
