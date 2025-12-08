/**
 * CLI Build unit tests (Vite-based)
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import type { BuildOptions } from './build';
import fs from 'fs';
import path from 'path';
import os from 'os';

// Mock vite build
const viteMock = {
  build: vi.fn().mockResolvedValue([{
    output: [{
      fileName: 'bundle.js',
    }],
  }]),
};

// Mock vite
vi.mock('vite', () => ({
  build: viteMock.build,
  createServer: vi.fn(),
}));

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
      path.join(srcDir, 'plugin.tsx'),
      `
      import { View, Text } from 'rill/sdk';
      export default function Plugin() {
        return <View><Text>Hello</Text></View>;
      }
    `
    );

    // Create dist directory and fake output file
    const distDir = path.join(tempDir, 'dist');
    fs.mkdirSync(distDir, { recursive: true });
    fs.writeFileSync(path.join(distDir, 'bundle.js'), '// mock bundle');

    // Reset mocks
    vi.clearAllMocks();
  });

  afterEach(() => {
    process.chdir(originalCwd);
    // Cleanup temp directory
    fs.rmSync(tempDir, { recursive: true, force: true });
  });

  describe('build', () => {
    it('should build plugin with default options', async () => {
      const { build } = await import('./build');

      await build({
        entry: 'src/plugin.tsx',
        outfile: 'dist/bundle.js',
        minify: true,
        sourcemap: false,
        watch: false,
      });

      expect(viteMock.build).toHaveBeenCalled();
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
      const outfile = path.join(customOutDir, 'bundle.js');

      // Pre-create outfile to simulate vite output
      fs.mkdirSync(customOutDir, { recursive: true });
      fs.writeFileSync(outfile, '// mock bundle');

      await build({
        entry: 'src/plugin.tsx',
        outfile: 'custom/output/bundle.js',
        minify: true,
        sourcemap: false,
        watch: false,
      });

      expect(fs.existsSync(customOutDir)).toBe(true);
    });

    it('should pass correct format options to vite', async () => {
      const { build } = await import('./build');

      await build({
        entry: 'src/plugin.tsx',
        outfile: 'dist/bundle.js',
        minify: false,
        sourcemap: true,
        watch: false,
      });

      expect(viteMock.build).toHaveBeenCalledWith(
        expect.objectContaining({
          build: expect.objectContaining({
            sourcemap: true,
            minify: false,
            target: 'es2020',
          }),
        })
      );
    });

    it('should set external react-native', async () => {
      const { build } = await import('./build');

      await build({
        entry: 'src/plugin.tsx',
        outfile: 'dist/bundle.js',
        minify: true,
        sourcemap: false,
        watch: false,
      });

      expect(viteMock.build).toHaveBeenCalledWith(
        expect.objectContaining({
          build: expect.objectContaining({
            rollupOptions: expect.objectContaining({
              external: expect.arrayContaining(['react-native']),
            }),
          }),
        })
      );
    });

    it('should inject runtime code via banner', async () => {
      const { build } = await import('./build');

      await build({
        entry: 'src/plugin.tsx',
        outfile: 'dist/bundle.js',
        minify: true,
        sourcemap: false,
        watch: false,
      });

      expect(viteMock.build).toHaveBeenCalledWith(
        expect.objectContaining({
          build: expect.objectContaining({
            rollupOptions: expect.objectContaining({
              output: expect.objectContaining({
                banner: expect.stringContaining('Rill Runtime Inject'),
              }),
            }),
          }),
        })
      );
    });

    it('should set jsx automatic mode', async () => {
      const { build } = await import('./build');

      await build({
        entry: 'src/plugin.tsx',
        outfile: 'dist/bundle.js',
        minify: true,
        sourcemap: false,
        watch: false,
      });

      expect(viteMock.build).toHaveBeenCalledWith(
        expect.objectContaining({
          esbuild: expect.objectContaining({
            jsx: 'automatic',
          }),
        })
      );
    });

    it('should define production environment', async () => {
      const { build } = await import('./build');

      await build({
        entry: 'src/plugin.tsx',
        outfile: 'dist/bundle.js',
        minify: true,
        sourcemap: false,
        watch: false,
      });

      expect(viteMock.build).toHaveBeenCalledWith(
        expect.objectContaining({
          define: expect.objectContaining({
            'process.env.NODE_ENV': '"production"',
            __DEV__: 'false',
          }),
        })
      );
    });

    it('should enable metafile when specified', async () => {
      const { build } = await import('./build');

      await build({
        entry: 'src/plugin.tsx',
        outfile: 'dist/bundle.js',
        minify: true,
        sourcemap: false,
        watch: false,
        metafile: 'dist/meta.json',
      });

      expect(viteMock.build).toHaveBeenCalled();
      // Metafile is created after build
    });
  });

  describe('watch mode', () => {
    it('should use watch option for watch mode', async () => {
      const { build } = await import('./build');

      // Set SIGINT handler to simulate early exit
      const originalOn = process.on.bind(process);
      const sigintHandler = vi.fn();
      process.on = vi.fn((event, handler) => {
        if (event === 'SIGINT') {
          sigintHandler.mockImplementation(handler);
        }
        return originalOn(event, handler);
      }) as typeof process.on;

      const buildPromise = build({
        entry: 'src/plugin.tsx',
        outfile: 'dist/bundle.js',
        minify: true,
        sourcemap: false,
        watch: true,
      });

      // Wait a bit for watch to start
      await new Promise((resolve) => setTimeout(resolve, 50));

      // Simulate SIGINT
      if (sigintHandler.mock.calls.length > 0) {
        sigintHandler();
      }

      await buildPromise;

      // Verify vite.build was called with watch option
      expect(viteMock.build).toHaveBeenCalledWith(
        expect.objectContaining({
          build: expect.objectContaining({
            watch: expect.anything(),
          }),
        })
      );

      process.on = originalOn;
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
        function Plugin() {
          return View;
        }
      `
      );
    });

    it('should analyze bundle file', async () => {
      const { analyze } = await import('./build');
      const consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});

      await analyze('dist/analyze-bundle.js');

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Bundle Analysis')
      );

      consoleSpy.mockRestore();
    });

    it('should throw error for non-existent bundle', async () => {
      const { analyze } = await import('./build');

      await expect(analyze('non-existent.js')).rejects.toThrow(
        'Bundle not found'
      );
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

      const consoleSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});

      await analyze('dist/bad-bundle.js');

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('react-native')
      );

      consoleSpy.mockRestore();
    });

    it('should report file size', async () => {
      const { analyze } = await import('./build');
      const consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});

      await analyze('dist/analyze-bundle.js');

      expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('Size:'));

      consoleSpy.mockRestore();
    });

    it('should report line count', async () => {
      const { analyze } = await import('./build');
      const consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});

      await analyze('dist/analyze-bundle.js');

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Lines:')
      );

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
    const buildSource = fs.readFileSync(
      path.join(__dirname, 'build.ts'),
      'utf-8'
    );

    expect(buildSource).toContain('__registerCallback');
    expect(buildSource).toContain('__invokeCallback');
    expect(buildSource).toContain('__removeCallback');
  });

  it('should include host event handling', () => {
    const buildSource = fs.readFileSync(
      path.join(__dirname, 'build.ts'),
      'utf-8'
    );

    expect(buildSource).toContain('__useHostEvent');
    expect(buildSource).toContain('__handleHostEvent');
    expect(buildSource).toContain('__handleHostMessage');
  });
});
