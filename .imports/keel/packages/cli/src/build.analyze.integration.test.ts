import { describe, it, expect, beforeEach } from 'bun:test';
import fs from 'fs';
import path from 'path';
import os from 'os';

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

  it('should build a guest with rill/sdk and pass strict guard', async () => {
    // Prepare minimal guest
    fs.writeFileSync(
      path.join('src', 'guest.tsx'),
      `import * as React from 'react';
import { View, Text } from 'rill/sdk';
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
      whitelist: ['react','react-native','react/jsx-runtime','rill/reconciler'],
      failOnViolation: true,
      treatEvalAsViolation: true,
      treatDynamicNonLiteralAsViolation: true,
    });
  });
});
