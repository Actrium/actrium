import { describe, it, expect, mock, beforeEach ,  spyOn } from 'bun:test';
import path from 'path';
import fs from 'fs';

describe('CLI Analyze - more string/comment edges', () => {
  const distDir = path.join(process.cwd(), 'dist');
  const bundle = (name: string) => path.join(distDir, name);

  beforeEach(() => {
    fs.mkdirSync(distDir, { recursive: true });
  });

  it('should not warn for import mentioned in comments', async () => {
    const { analyze } = await import('./build');
    const code = `// import('left-pad') in comment\n/* require('lodash') */\nconsole.log('ok');`;
    fs.writeFileSync(bundle('comment.js'), code);
    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
    await analyze('dist/comment.js', { whitelist: ['react'], failOnViolation: false });
    // should not include non-whitelisted warning from comments
    expect(warnSpy).not.toHaveBeenCalledWith(expect.stringContaining('non-whitelisted modules'));
    warnSpy.mockRestore();
  });

  it('should not warn for require/import inside plain strings', async () => {
    const { analyze } = await import('./build');
    const code = `const a = "require('left-pad')"; const b = 'import(\'lodash\')';`;
    fs.writeFileSync(bundle('strings.js'), code);
    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
    await analyze('dist/strings.js', { whitelist: ['react'], failOnViolation: false });
    expect(warnSpy).not.toHaveBeenCalledWith(expect.stringContaining('non-whitelisted modules'));
    warnSpy.mockRestore();
  });

  it('should still warn for real dynamic import call', async () => {
    const { analyze } = await import('./build');
    const code = `import('left-pad');`;
    fs.writeFileSync(bundle('real-dyn.js'), code);
    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
    await analyze('dist/real-dyn.js', { whitelist: ['react'], failOnViolation: false });
    expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('non-whitelisted modules'));
    warnSpy.mockRestore();
  });
});
