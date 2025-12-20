import { afterEach, beforeEach, describe, expect, it, spyOn } from 'bun:test';
import fs from 'fs';
import os from 'os';
import path from 'path';

describe('CLI Analyze - more string/comment edges', () => {
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

  it('should not warn for import mentioned in comments', async () => {
    const { analyze } = await import('./build');
    const code = `// import('left-pad') in comment\n/* require('lodash') */\nconsole.log('ok');`;
    const bundlePath = bundle('comment.js');
    fs.writeFileSync(bundlePath, code);
    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
    await analyze(bundlePath, { whitelist: ['react'], failOnViolation: false });
    // should not include non-whitelisted warning from comments
    expect(warnSpy).not.toHaveBeenCalledWith(expect.stringContaining('non-whitelisted modules'));
    warnSpy.mockRestore();
  });

  it('should not warn for require/import inside plain strings', async () => {
    const { analyze } = await import('./build');
    const code = `const a = "require('left-pad')"; const b = 'import('lodash')';`;
    const bundlePath = bundle('strings.js');
    fs.writeFileSync(bundlePath, code);
    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
    await analyze(bundlePath, { whitelist: ['react'], failOnViolation: false });
    expect(warnSpy).not.toHaveBeenCalledWith(expect.stringContaining('non-whitelisted modules'));
    warnSpy.mockRestore();
  });

  it('should still warn for real dynamic import call', async () => {
    const { analyze } = await import('./build');
    const code = `import('left-pad');`;
    const bundlePath = bundle('real-dyn.js');
    fs.writeFileSync(bundlePath, code);
    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
    await analyze(bundlePath, { whitelist: ['react'], failOnViolation: false });
    expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('non-whitelisted modules'));
    warnSpy.mockRestore();
  });
});
