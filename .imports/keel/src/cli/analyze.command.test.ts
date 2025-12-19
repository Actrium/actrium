import { afterEach, beforeEach, describe, expect, it, spyOn } from 'bun:test';
import fs from 'fs';
import path from 'path';

// Test the CLI analyze command handler (programmatic)
describe('CLI analyze command', () => {
  const distDir = path.join(process.cwd(), 'dist');
  const bundle = path.join(distDir, 'cli-analyze.js');
  let logSpy: ReturnType<typeof spyOn>;

  beforeEach(() => {
    fs.mkdirSync(distDir, { recursive: true });
    logSpy = spyOn(console, 'log').mockImplementation(() => {});
  });

  afterEach(() => {
    logSpy.mockRestore();
  });

  it('should report violations via CLI', async () => {
    const { analyze } = await import('./build');
    fs.writeFileSync(bundle, "require('left-pad');");

    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
    await analyze('dist/cli-analyze.js', { whitelist: ['react'], failOnViolation: false });
    expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('non-whitelisted modules'));
    warnSpy.mockRestore();
  });
});
