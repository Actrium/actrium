import { afterEach, beforeEach, describe, expect, it, spyOn } from 'bun:test';
import fs from 'fs';
import os from 'os';
import path from 'path';

describe('CLI Analyze - ignore patterns', () => {
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

  it('should ignore data URLs and http imports', async () => {
    const { analyze } = await import('./build');
    const code = `import('data:text/javascript,export default 1'); import('https://cdn.example.com/x.js');`;
    const bundlePath = bundle('urls.js');
    fs.writeFileSync(bundlePath, code);
    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
    await analyze(bundlePath, { whitelist: ['react'], failOnViolation: false });
    expect(warnSpy).not.toHaveBeenCalledWith(expect.stringContaining('non-whitelisted modules'));
    warnSpy.mockRestore();
  });

  it('should ignore import.meta usage', async () => {
    const { analyze } = await import('./build');
    const code = `console.log(import.meta.url);`;
    const bundlePath = bundle('importmeta.js');
    fs.writeFileSync(bundlePath, code);
    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
    await analyze(bundlePath, { whitelist: ['react'], failOnViolation: false });
    expect(warnSpy).not.toHaveBeenCalled();
    warnSpy.mockRestore();
  });

  it('should not warn for bundler injected calls like __webpack_require__', async () => {
    const { analyze } = await import('./build');
    const code = `__webpack_require__('left-pad'); __vite_ssr_import__('lodash');`;
    const bundlePath = bundle('bundler.js');
    fs.writeFileSync(bundlePath, code);
    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
    await analyze(bundlePath, { whitelist: ['react'], failOnViolation: false });
    expect(warnSpy).not.toHaveBeenCalled();
    warnSpy.mockRestore();
  });
});
