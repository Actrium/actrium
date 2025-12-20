import { afterEach, beforeEach, describe, expect, it, spyOn } from 'bun:test';
import fs from 'fs';
import os from 'os';
import path from 'path';

describe('CLI Analyze - edge cases', () => {
  let distDir: string;
  let bundle: (name: string) => string;
  let logSpy: ReturnType<typeof spyOn>;

  beforeEach(() => {
    distDir = fs.mkdtempSync(path.join(os.tmpdir(), 'rill-cli-test-'));
    bundle = (name: string) => path.join(distDir, name);
    logSpy = spyOn(console, 'log').mockImplementation(() => {});
  });

  afterEach(() => {
    logSpy.mockRestore();
    fs.rmSync(distDir, { recursive: true, force: true });
  });

  it('should detect dynamic import specifier', async () => {
    const { analyze } = await import('./build');
    const bundlePath = bundle('dyn.js');
    fs.writeFileSync(bundlePath, `import('left-pad');`);
    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
    await analyze(bundlePath, { whitelist: ['react'], failOnViolation: false });
    expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('non-whitelisted modules'));
    warnSpy.mockRestore();
  });

  it('should ignore relative dynamic import', async () => {
    const { analyze } = await import('./build');
    const bundlePath = bundle('dyn-rel.js');
    fs.writeFileSync(bundlePath, `import('./local');`);
    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
    await analyze(bundlePath, { whitelist: ['react'], failOnViolation: false });
    // should not warn for relative
    expect(warnSpy).not.toHaveBeenCalledWith(expect.stringContaining('non-whitelisted modules'));
    warnSpy.mockRestore();
  });
});
