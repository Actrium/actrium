import { afterEach, beforeEach, describe, expect, it, spyOn } from 'bun:test';
import fs from 'fs';
import os from 'os';
import path from 'path';

// Test the CLI analyze command handler (programmatic)
describe('CLI analyze command', () => {
  let distDir: string;
  let bundle: string;
  let logSpy: ReturnType<typeof spyOn>;

  beforeEach(() => {
    distDir = fs.mkdtempSync(path.join(os.tmpdir(), 'rill-cli-test-'));
    bundle = path.join(distDir, 'cli-analyze.js');
    logSpy = spyOn(console, 'log').mockImplementation(() => {});
  });

  afterEach(() => {
    logSpy.mockRestore();
    fs.rmSync(distDir, { recursive: true, force: true });
  });

  it('should report violations via CLI', async () => {
    const { analyze } = await import('./build');
    fs.writeFileSync(bundle, "require('left-pad');");

    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
    await analyze(bundle, { whitelist: ['react'], failOnViolation: false });
    expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('non-whitelisted modules'));
    warnSpy.mockRestore();
  });
});
