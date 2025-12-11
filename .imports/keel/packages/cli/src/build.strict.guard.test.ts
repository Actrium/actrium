import { describe, it, expect, beforeEach, spyOn } from 'bun:test';
import fs from 'fs';
import path from 'path';

describe('Strict Guard - violation cases', () => {
  const distDir = path.join(process.cwd(), 'dist');
  const file = (name: string) => path.join(distDir, name);

  beforeEach(() => {
    fs.mkdirSync(distDir, { recursive: true });
  });

  it('should fail when bundle contains rill/sdk at runtime', async () => {
    const { analyze } = await import('./build');
    fs.writeFileSync(file('guard-sdk.js'), "require('rill/sdk');");

    await expect(
      analyze('dist/guard-sdk.js', {
        whitelist: ['react','react-native','react/jsx-runtime','rill/reconciler'],
        failOnViolation: true,
        treatEvalAsViolation: true,
        treatDynamicNonLiteralAsViolation: true,
      })
    ).rejects.toThrow('Analyze failed');
  });

  it('should pass when bundle only contains whitelisted deps', async () => {
    const { analyze } = await import('./build');
    fs.writeFileSync(file('guard-ok.js'), "require('react'); require('rill/reconciler');");
    const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});

    await analyze('dist/guard-ok.js', {
      whitelist: ['react','react-native','react/jsx-runtime','rill/reconciler'],
      failOnViolation: true,
    });

    expect(warnSpy).not.toHaveBeenCalled();
    warnSpy.mockRestore();
  });
});
