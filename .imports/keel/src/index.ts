/**
 * keel (root entry)
 *
 * This package enforces explicit entrypoints:
 * - Host:  `keel/host`
 * - Guest: `keel/guest`
 *
 * Do not import from `keel`.
 */

throw new Error(
  [
    "[keel] `import ... from 'keel'` is not supported.",
    'Use one of these explicit entrypoints:',
    "  - Host:  `import { Engine } from 'keel/host'`",
    "  - Guest: `import { View, Text } from 'keel/guest'`",
    "  - Contract: `import { defineKeelContract } from 'keel/contract'`",
    "  - UI preset: `import { EngineView } from 'keel/host/preset'`",
  ].join('\n')
);
