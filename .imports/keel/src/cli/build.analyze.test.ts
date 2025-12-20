import { afterEach, beforeEach, describe, expect, it, spyOn } from 'bun:test';
import fs from 'fs';
import os from 'os';
import path from 'path';

describe('CLI Analyze - whitelist scan', () => {
  let tmpDir: string;
  let bundle: string;
  let logSpy: ReturnType<typeof spyOn>;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'rill-cli-test-'));
    bundle = path.join(tmpDir, 'scan-bundle.js');
    logSpy = spyOn(console, 'log').mockImplementation(() => {});
  });

  afterEach(() => {
    logSpy.mockRestore();
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('should warn for non-whitelisted modules', async () => {
    const { analyze } = await import('./build');
    fs.writeFileSync(bundle, `require('left-pad');`);
    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
    await analyze(bundle, { whitelist: ['react'], failOnViolation: false });
    expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('non-whitelisted modules'));
    warnSpy.mockRestore();
  });

  it('should fail when failOnViolation is true', async () => {
    const { analyze } = await import('./build');
    fs.writeFileSync(bundle, `import x from 'lodash';`);
    await expect(
      analyze(bundle, { whitelist: ['react'], failOnViolation: true })
    ).rejects.toThrow('Found non-whitelisted modules: lodash');
  });

  it('should ignore relative imports', async () => {
    const { analyze } = await import('./build');
    fs.writeFileSync(bundle, `import x from './local';`);
    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
    await analyze(bundle, { whitelist: ['react'], failOnViolation: false });
    // no warnings for relative path
    expect(warnSpy).not.toHaveBeenCalledWith(expect.stringContaining('non-whitelisted modules'));
    warnSpy.mockRestore();
  });
});
