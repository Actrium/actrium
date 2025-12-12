/**
 * CLI Build unit tests (Vite-based)
 *
 * Tests for the build CLI commands. Uses actual vite for integration testing.
 */

import { afterEach, beforeEach, describe, expect, it, spyOn } from 'bun:test';
import fs from 'fs';
import os from 'os';
import path from 'path';
import type { BuildOptions } from './build';

describe('CLI Build', () => {
  let tempDir: string;
  let originalCwd: string;

  beforeEach(() => {
    // Create temp directory
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'rill-test-'));
    originalCwd = process.cwd();
    process.chdir(tempDir);

    // Create test entry file
    const srcDir = path.join(tempDir, 'src');
    fs.mkdirSync(srcDir, { recursive: true });
    fs.writeFileSync(
      path.join(srcDir, 'guest.tsx'),
      `
      import { View, Text } from '@rill/core/sdk';
      export default function Guest() {
        return <View><Text>Hello</Text></View>;
      }
    `
    );

    // Create dist directory
    const distDir = path.join(tempDir, 'dist');
    fs.mkdirSync(distDir, { recursive: true });
  });

  afterEach(() => {
    process.chdir(originalCwd);
    // Cleanup temp directory
    fs.rmSync(tempDir, { recursive: true, force: true });
  });

  describe('build', () => {
    it('should build guest with default options', async () => {
      const { build } = await import('./build');

      // This test verifies the build function doesn't throw
      // The actual vite build is called
      await build({
        entry: 'src/guest.tsx',
        outfile: 'dist/bundle.js',
        minify: true,
        sourcemap: false,
        watch: false,
      });

      // Verify output file exists
      expect(fs.existsSync(path.join(tempDir, 'dist/bundle.js'))).toBe(true);
    });

    it('should throw error for non-existent entry file', async () => {
      const { build } = await import('./build');

      await expect(
        build({
          entry: 'non-existent.tsx',
          outfile: 'dist/bundle.js',
          minify: true,
          sourcemap: false,
          watch: false,
        })
      ).rejects.toThrow('Entry file not found');
    });

    it('should create output directory if not exists', async () => {
      const { build } = await import('./build');
      const customOutDir = path.join(tempDir, 'custom', 'output');

      await build({
        entry: 'src/guest.tsx',
        outfile: 'custom/output/bundle.js',
        minify: true,
        sourcemap: false,
        watch: false,
      });

      expect(fs.existsSync(customOutDir)).toBe(true);
    });
  });

  describe('analyze', () => {
    beforeEach(() => {
      // Create a fake bundle file
      const distDir = path.join(tempDir, 'dist');
      fs.mkdirSync(distDir, { recursive: true });
      fs.writeFileSync(
        path.join(distDir, 'analyze-bundle.js'),
        `
        // Bundle content
        var View = "View";
        var Text = "Text";
        function Guest() {
          return View;
        }
      `
      );
    });

    it('should analyze bundle file', async () => {
      const { analyze } = await import('./build');
      const consoleSpy = spyOn(console, 'log').mockImplementation(() => {});

      await analyze('dist/analyze-bundle.js');

      expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('Bundle Analysis'));

      consoleSpy.mockRestore();
    });

    it('should throw error for non-existent bundle', async () => {
      const { analyze } = await import('./build');

      await expect(analyze('non-existent.js')).rejects.toThrow('Bundle not found');
    });

    it('should warn about react-native references', async () => {
      const { analyze } = await import('./build');
      const distDir = path.join(tempDir, 'dist');
      fs.writeFileSync(
        path.join(distDir, 'bad-bundle.js'),
        `
        import { View } from 'react-native';
      `
      );

      const consoleSpy = spyOn(console, 'warn').mockImplementation(() => {});

      await analyze('dist/bad-bundle.js');

      expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('react-native'));

      consoleSpy.mockRestore();
    });

    it('should report file size', async () => {
      const { analyze } = await import('./build');
      const consoleSpy = spyOn(console, 'log').mockImplementation(() => {});

      await analyze('dist/analyze-bundle.js');

      expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('Size:'));

      consoleSpy.mockRestore();
    });

    it('should report line count', async () => {
      const { analyze } = await import('./build');
      const consoleSpy = spyOn(console, 'log').mockImplementation(() => {});

      await analyze('dist/analyze-bundle.js');

      expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('Lines:'));

      consoleSpy.mockRestore();
    });
  });
});

describe('Build Options Validation', () => {
  it('should have correct BuildOptions interface', () => {
    const options: BuildOptions = {
      entry: 'src/index.tsx',
      outfile: 'dist/bundle.js',
      minify: true,
      sourcemap: false,
      watch: false,
    };

    expect(options.entry).toBe('src/index.tsx');
    expect(options.outfile).toBe('dist/bundle.js');
    expect(options.minify).toBe(true);
    expect(options.sourcemap).toBe(false);
    expect(options.watch).toBe(false);
  });

  it('should accept optional metafile', () => {
    const options: BuildOptions = {
      entry: 'src/index.tsx',
      outfile: 'dist/bundle.js',
      minify: true,
      sourcemap: false,
      watch: false,
      metafile: 'dist/meta.json',
    };

    expect(options.metafile).toBe('dist/meta.json');
  });
});

describe('Runtime Inject Content', () => {
  it('should include callback registration functions', () => {
    // 读取源码验证运行时注入内容
    const buildSource = fs.readFileSync(path.join(__dirname, 'build.ts'), 'utf-8');

    expect(buildSource).toContain('__registerCallback');
    expect(buildSource).toContain('__invokeCallback');
    expect(buildSource).toContain('__removeCallback');
  });

  it('should include host event handling', () => {
    const buildSource = fs.readFileSync(path.join(__dirname, 'build.ts'), 'utf-8');

    expect(buildSource).toContain('__useHostEvent');
    expect(buildSource).toContain('__handleHostEvent');
    expect(buildSource).toContain('__handleHostMessage');
  });
});
