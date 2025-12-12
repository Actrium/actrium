import { beforeEach, describe, expect, it, spyOn } from 'bun:test';
import fs from 'fs';
import path from 'path';

describe('CLI Analyze - ignore patterns', () => {
  const distDir = path.join(process.cwd(), 'dist');
  const bundle = (name: string) => path.join(distDir, name);

  beforeEach(() => {
    fs.mkdirSync(distDir, { recursive: true });
  });

  it('should ignore data URLs and http imports', async () => {
    const { analyze } = await import('./build');
    const code = `import('data:text/javascript,export default 1'); import('https://cdn.example.com/x.js');`;
    fs.writeFileSync(bundle('urls.js'), code);
    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
    await analyze('dist/urls.js', { whitelist: ['react'], failOnViolation: false });
    expect(warnSpy).not.toHaveBeenCalledWith(expect.stringContaining('non-whitelisted modules'));
    warnSpy.mockRestore();
  });

  it('should ignore import.meta usage', async () => {
    const { analyze } = await import('./build');
    const code = `console.log(import.meta.url);`;
    fs.writeFileSync(bundle('importmeta.js'), code);
    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
    await analyze('dist/importmeta.js', { whitelist: ['react'], failOnViolation: false });
    expect(warnSpy).not.toHaveBeenCalled();
    warnSpy.mockRestore();
  });

  it('should not warn for bundler injected calls like __webpack_require__', async () => {
    const { analyze } = await import('./build');
    const code = `__webpack_require__('left-pad'); __vite_ssr_import__('lodash');`;
    fs.writeFileSync(bundle('bundler.js'), code);
    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
    await analyze('dist/bundler.js', { whitelist: ['react'], failOnViolation: false });
    expect(warnSpy).not.toHaveBeenCalled();
    warnSpy.mockRestore();
  });
});
