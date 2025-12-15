import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import fs from 'fs';
import os from 'os';
import path from 'path';

// Integration test: run full build via build() then run analyze() on the output
// Ensures that a typical guest using rill/sdk gets inlined and passes strict guard.
describe('Analyze integration - build then analyze', () => {
  let tempDir: string;
  let originalCwd: string;

  beforeEach(() => {
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'rill-ana-'));
    originalCwd = process.cwd();
    process.chdir(tempDir);
    fs.mkdirSync('src', { recursive: true });
  });

  afterEach(() => {
    process.chdir(originalCwd);
    fs.rmSync(tempDir, { recursive: true, force: true });
  });

  it('should build a guest with rill/sdk and pass strict guard', async () => {
    // Prepare minimal guest
    fs.writeFileSync(
      path.join('src', 'guest.tsx'),
      `import * as React from 'react';
import { View, Text } from '@rill/core/sdk';
export default function Guest(){
  return <View><Text>OK</Text></View>;
}
`
    );

    const { build, analyze } = await import('./build');

    await build({
      entry: 'src/guest.tsx',
      outfile: 'dist/bundle.js',
      minify: true,
      sourcemap: false,
      watch: false,
      strict: true,
    });

    // Ensure file exists
    const bundlePath = path.join('dist', 'bundle.js');
    expect(fs.existsSync(bundlePath)).toBe(true);

    // Now run analyze manually (should pass and not throw)
    await analyze(bundlePath, {
      whitelist: [
        'react',
        'react-native',
        'react/jsx-runtime',
        '@rill/core',
        '@rill/core/sdk',
        'rill/reconciler',
      ],
      failOnViolation: true,
      treatEvalAsViolation: true,
      treatDynamicNonLiteralAsViolation: true,
    });
  });
});
