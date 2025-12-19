import { afterEach, beforeEach, describe, expect, it, spyOn } from 'bun:test';
import fs from 'fs';
import path from 'path';

describe('CLI Analyze - whitelist scan', () => {
  const tmpDir = path.join(process.cwd(), 'dist');
  const bundle = path.join(tmpDir, 'scan-bundle.js');
  let logSpy: ReturnType<typeof spyOn>;

  beforeEach(() => {
    fs.mkdirSync(tmpDir, { recursive: true });
    logSpy = spyOn(console, 'log').mockImplementation(() => {});
  });

  afterEach(() => {
    logSpy.mockRestore();
  });

  it('should warn for non-whitelisted modules', async () => {
    const { analyze } = await import('./build');
    fs.writeFileSync(bundle, `require('left-pad');`);
    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
    await analyze('dist/scan-bundle.js', { whitelist: ['react'], failOnViolation: false });
    expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('non-whitelisted modules'));
    warnSpy.mockRestore();
  });

  it('should fail when failOnViolation is true', async () => {
    const { analyze } = await import('./build');
    fs.writeFileSync(bundle, `import x from 'lodash';`);
    await expect(
      analyze('dist/scan-bundle.js', { whitelist: ['react'], failOnViolation: true })
    ).rejects.toThrow('Found non-whitelisted modules: lodash');
  });

  it('should ignore relative imports', async () => {
    const { analyze } = await import('./build');
    fs.writeFileSync(bundle, `import x from './local';`);
    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
    await analyze('dist/scan-bundle.js', { whitelist: ['react'], failOnViolation: false });
    // no warnings for relative path
    expect(warnSpy).not.toHaveBeenCalledWith(expect.stringContaining('non-whitelisted modules'));
    warnSpy.mockRestore();
  });
});
