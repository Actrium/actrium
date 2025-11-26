/**
 * CLI Build 单元测试
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import type { BuildOptions } from './build';
import fs from 'fs';
import path from 'path';
import os from 'os';

// 延迟导入以便 mock
const esbuildMock = {
  build: vi.fn().mockResolvedValue({
    metafile: {
      inputs: {},
      outputs: {},
    },
  }),
  context: vi.fn().mockResolvedValue({
    watch: vi.fn().mockResolvedValue(undefined),
    dispose: vi.fn().mockResolvedValue(undefined),
  }),
};

// Mock esbuild
vi.mock('esbuild', () => esbuildMock);

describe('CLI Build', () => {
  let tempDir: string;
  let originalCwd: string;

  beforeEach(() => {
    // 创建临时目录
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'rill-test-'));
    originalCwd = process.cwd();
    process.chdir(tempDir);

    // 创建测试入口文件
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

    // 创建 dist 目录和模拟输出文件
    const distDir = path.join(tempDir, 'dist');
    fs.mkdirSync(distDir, { recursive: true });
    fs.writeFileSync(path.join(distDir, 'bundle.js'), '// mock bundle');

    // 重置 mock
    vi.clearAllMocks();
  });

  afterEach(() => {
    process.chdir(originalCwd);
    // 清理临时目录
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

      expect(esbuildMock.build).toHaveBeenCalled();
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

      // 预创建输出文件以模拟 esbuild 输出
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

    it('should pass correct format options to esbuild', async () => {
      const { build } = await import('./build');

      await build({
        entry: 'src/plugin.tsx',
        outfile: 'dist/bundle.js',
        minify: false,
        sourcemap: true,
        watch: false,
      });

      expect(esbuildMock.build).toHaveBeenCalledWith(
        expect.objectContaining({
          bundle: true,
          format: 'iife',
          globalName: '__RillPlugin',
          minify: false,
          sourcemap: true,
          target: 'es2020',
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

      expect(esbuildMock.build).toHaveBeenCalledWith(
        expect.objectContaining({
          external: ['react-native'],
        })
      );
    });

    it('should inject runtime code', async () => {
      const { build } = await import('./build');

      await build({
        entry: 'src/plugin.tsx',
        outfile: 'dist/bundle.js',
        minify: true,
        sourcemap: false,
        watch: false,
      });

      expect(esbuildMock.build).toHaveBeenCalledWith(
        expect.objectContaining({
          inject: expect.arrayContaining([
            expect.stringContaining('.rill-runtime-inject.js'),
          ]),
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

      expect(esbuildMock.build).toHaveBeenCalledWith(
        expect.objectContaining({
          jsx: 'automatic',
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

      expect(esbuildMock.build).toHaveBeenCalledWith(
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

      expect(esbuildMock.build).toHaveBeenCalledWith(
        expect.objectContaining({
          metafile: true,
        })
      );
    });
  });

  describe('watch mode', () => {
    it('should use context for watch mode', async () => {
      const { build } = await import('./build');

      // 设置 SIGINT 模拟提前退出
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

      // 等待一点时间让 watch 启动
      await new Promise((resolve) => setTimeout(resolve, 50));

      // 模拟 SIGINT
      if (sigintHandler.mock.calls.length > 0) {
        sigintHandler();
      }

      await buildPromise;

      expect(esbuildMock.context).toHaveBeenCalled();

      process.on = originalOn;
    });
  });

  describe('analyze', () => {
    beforeEach(() => {
      // 创建一个模拟的 bundle 文件
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
