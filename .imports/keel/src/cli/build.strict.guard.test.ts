import { afterEach, beforeEach, describe, expect, it, spyOn } from 'bun:test';
import fs from 'fs';
import path from 'path';

describe('Strict Guard - violation cases', () => {
  const distDir = path.join(process.cwd(), 'dist');
  const file = (name: string) => path.join(distDir, name);
  let logSpy: ReturnType<typeof spyOn>;

  beforeEach(() => {
    fs.mkdirSync(distDir, { recursive: true });
    logSpy = spyOn(console, 'log').mockImplementation(() => {});
  });

  afterEach(() => {
    logSpy.mockRestore();
  });

  it('should fail when bundle contains non-whitelisted module at runtime', async () => {
    const { analyze } = await import('./build');
    fs.writeFileSync(file('guard-lodash.js'), "require('lodash');");

    await expect(
      analyze('dist/guard-lodash.js', {
        whitelist: ['react', 'react-native', 'react/jsx-runtime', '@rill/let'],
        failOnViolation: true,
        treatEvalAsViolation: true,
        treatDynamicNonLiteralAsViolation: true,
      })
    ).rejects.toThrow('Found non-whitelisted modules: lodash');
  });

  it('should pass when bundle only contains whitelisted deps', async () => {
    const { analyze } = await import('./build');
    fs.writeFileSync(file('guard-ok.js'), "require('react'); require('@rill/let');");
    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});

    await analyze('dist/guard-ok.js', {
      whitelist: ['react', 'react-native', 'react/jsx-runtime', '@rill/let'],
      failOnViolation: true,
    });

    expect(warnSpy).not.toHaveBeenCalled();
    warnSpy.mockRestore();
  });
});
