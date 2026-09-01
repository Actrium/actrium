// NOTE: RN runtime may not provide Node.js `process` at runtime, but some shared code
// (e.g. keel) uses `process.versions` for environment detection.
// Keep the type broad and compatible with @types/node.

declare const process: NodeJS.Process | undefined;
declare var __ACTRIUM_ACTR_GATEWAY_URL__: string | undefined;
