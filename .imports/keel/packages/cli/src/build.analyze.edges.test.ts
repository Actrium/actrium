import { describe, it, expect, mock, beforeEach ,  spyOn } from 'bun:test';
import path from 'path';
import fs from 'fs';

describe('CLI Analyze - edge cases', () => {
  const distDir = path.join(process.cwd(), 'dist');
  const bundle = (name: string) => path.join(distDir, name);

  beforeEach(() => {
    fs.mkdirSync(distDir, { recursive: true });
  });

  it('should detect dynamic import specifier', async () => {
    const { analyze } = await import('./build');
    fs.writeFileSync(bundle('dyn.js'), `import('left-pad');`);
    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
    await analyze('dist/dyn.js', { whitelist: ['react'], failOnViolation: false });
    expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('non-whitelisted modules'));
    warnSpy.mockRestore();
  });

  it('should ignore relative dynamic import', async () => {
    const { analyze } = await import('./build');
    fs.writeFileSync(bundle('dyn-rel.js'), `import('./local');`);
    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
    await analyze('dist/dyn-rel.js', { whitelist: ['react'], failOnViolation: false });
    // should not warn for relative
    expect(warnSpy).not.toHaveBeenCalledWith(expect.stringContaining('non-whitelisted modules'));
    warnSpy.mockRestore();
  });
});
