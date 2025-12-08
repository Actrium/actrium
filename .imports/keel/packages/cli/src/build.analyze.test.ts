import { describe, it, expect, vi, beforeEach } from 'vitest';
import path from 'path';
import fs from 'fs';

describe('CLI Analyze - whitelist scan', () => {
  const tmpDir = path.join(process.cwd(), 'dist');
  const bundle = path.join(tmpDir, 'scan-bundle.js');

  beforeEach(() => {
    fs.mkdirSync(tmpDir, { recursive: true });
  });

  it('should warn for non-whitelisted modules', async () => {
    const { analyze } = await import('./build');
    fs.writeFileSync(bundle, `require('left-pad');`);
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
    await analyze('dist/scan-bundle.js', { whitelist: ['react'], failOnViolation: false });
    expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('non-whitelisted modules'));
    warnSpy.mockRestore();
  });

  it('should fail when failOnViolation is true', async () => {
    const { analyze } = await import('./build');
    fs.writeFileSync(bundle, `import x from 'lodash';`);
    await expect(analyze('dist/scan-bundle.js', { whitelist: ['react'], failOnViolation: true }))
      .rejects.toThrow('Analyze failed');
  });

  it('should ignore relative imports', async () => {
    const { analyze } = await import('./build');
    fs.writeFileSync(bundle, `import x from './local';`);
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
    await analyze('dist/scan-bundle.js', { whitelist: ['react'], failOnViolation: false });
    // no warnings for relative path
    expect(warnSpy).not.toHaveBeenCalledWith(expect.stringContaining('non-whitelisted modules'));
    warnSpy.mockRestore();
  });
});
