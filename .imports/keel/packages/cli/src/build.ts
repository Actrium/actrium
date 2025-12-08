/**
 * Rill CLI - Build
 *
 * Vite-based plugin bundler (replaced esbuild)
 */

import { build as viteBuild } from 'vite';
import type { InlineConfig } from 'vite';
import path from 'path';
import fs from 'fs';

/**
 * Build options
 */
export interface BuildOptions {
  /**
   * Entry file path
   */
  entry: string;

  /**
   * Output file path
   * @default 'dist/bundle.js'
   */
  outfile: string;

  /**
   * Enable minification
   * @default true
   */
  minify: boolean;

  /**
   * Generate sourcemap
   * @default false
   */
  sourcemap: boolean;

  /**
   * Enable watch mode
   * @default false
   */
  watch: boolean;

  /**
   * Metadata output path
   */
  metafile?: string;
}

/**
 * Runtime injection code
 * Sets up necessary global environment before bundle execution
 */
const RUNTIME_INJECT = `
// Rill Runtime Inject
(function() {
  'use strict';

  // Callback registry
  var __callbacks = new Map();
  var __callbackId = 0;

  // Register callback
  globalThis.__registerCallback = function(fn) {
    var id = 'fn_' + (++__callbackId) + '_' + Date.now().toString(36);
    __callbacks.set(id, fn);
    return id;
  };

  // Invoke callback
  globalThis.__invokeCallback = function(fnId, args) {
    var fn = __callbacks.get(fnId);
    if (fn) {
      try {
        return fn.apply(null, args || []);
      } catch (e) {
        console.error('[rill] Callback error:', e);
      }
    } else {
      console.warn('[rill] Callback not found:', fnId);
    }
  };

  // Remove callback
  globalThis.__removeCallback = function(fnId) {
    __callbacks.delete(fnId);
  };

  // Host event listeners
  var __hostEventListeners = new Map();

  // Register host event listener
  globalThis.__useHostEvent = function(eventName, callback) {
    if (!__hostEventListeners.has(eventName)) {
      __hostEventListeners.set(eventName, new Set());
    }
    __hostEventListeners.get(eventName).add(callback);
  };

  // Handle host event
  globalThis.__handleHostEvent = function(eventName, payload) {
    var listeners = __hostEventListeners.get(eventName);
    if (listeners) {
      listeners.forEach(function(listener) {
        try {
          listener(payload);
        } catch (e) {
          console.error('[rill] Host event listener error:', e);
        }
      });
    }
  };

  // Handle host message
  globalThis.__handleHostMessage = function(message) {
    switch (message.type) {
      case 'CALL_FUNCTION':
        __invokeCallback(message.fnId, message.args);
        break;
      case 'HOST_EVENT':
        __handleHostEvent(message.eventName, message.payload);
        break;
      case 'CONFIG_UPDATE':
        if (globalThis.__config) {
          Object.assign(globalThis.__config, message.config);
        }
        break;
      case 'DESTROY':
        __callbacks.clear();
        __hostEventListeners.clear();
        break;
    }
  };

  // Config storage
  globalThis.__config = globalThis.__getConfig ? globalThis.__getConfig() : {};

})();
`;

/**
 * Auto-render footer code
 */
const AUTO_RENDER_FOOTER = `
/* Auto-render */
(function() {
  if (typeof __sendToHost === 'function' && typeof __RillPlugin !== 'undefined') {
    try {
      var React = typeof require === 'function' ? require('react') : (typeof globalThis.React !== 'undefined' ? globalThis.React : null);

      if (!React) {
        console.error('[rill] React not found, cannot auto-render');
        return;
      }

      // Import render function from reconciler
      // In Engine's mock runtime, this will be available via require polyfill
      var reconciler = typeof require === 'function' ? require('rill/reconciler') : null;

      if (!reconciler || !reconciler.render) {
        console.error('[rill] Reconciler not found, cannot auto-render');
        return;
      }

      // Get the default export (the main component)
      var Component = __RillPlugin.default;

      if (!Component) {
        console.warn('[rill] No default export found in plugin');
        return;
      }

      // Create React element and render via reconciler
      var element = React.createElement(Component);

      console.log('[rill] Auto-rendering plugin component');
      reconciler.render(element, __sendToHost);

    } catch (error) {
      console.error('[rill] Auto-render failed:', error);
    }
  }
})();

/* End of Rill Plugin Bundle */`;

/**
 * Execute build using Vite
 */
export async function build(options: BuildOptions): Promise<void> {
  const startTime = Date.now();

  const {
    entry,
    outfile,
    minify,
    sourcemap,
    watch,
    metafile,
  } = options;

  // Validate entry file
  const entryPath = path.resolve(process.cwd(), entry);
  if (!fs.existsSync(entryPath)) {
    console.error(`❌ Error: Entry file not found: ${entryPath}`);
    console.error(`\nTip: Please check if the file path is correct`);
    throw new Error(`Entry file not found: ${entryPath}`);
  }

  // Check file extension
  const ext = path.extname(entryPath);
  if (!['.ts', '.tsx', '.js', '.jsx'].includes(ext)) {
    console.warn(`⚠️  Warning: File extension "${ext}" may not be supported`);
    console.warn(`Recommended: .ts, .tsx, .js, .jsx`);
  }

  // Ensure output directory exists
  const outDir = path.dirname(path.resolve(process.cwd(), outfile));
  const outFileName = path.basename(outfile);
  if (!fs.existsSync(outDir)) {
    fs.mkdirSync(outDir, { recursive: true });
  }

  console.log(`Building ${entry}...`);

  // Vite configuration for plugin build
  const viteConfig: InlineConfig = {
    configFile: false,
    root: process.cwd(),
    mode: 'production',
    logLevel: 'warn',
    build: {
      lib: {
        entry: entryPath,
        name: '__RillPlugin',
        formats: ['iife'],
        fileName: () => outFileName.replace(/\.js$/, ''),
      },
      outDir,
      emptyOutDir: false,
      sourcemap,
      minify: minify ? 'esbuild' : false,
      target: 'es2020',
      rollupOptions: {
        external: ['react', 'react/jsx-runtime', 'react/jsx-dev-runtime', 'react-native', 'rill/reconciler'],
        output: {
          format: 'iife',
          name: '__RillPlugin',
          banner: `/* Rill Plugin Bundle - Generated by rill-cli */\n${RUNTIME_INJECT}`,
          footer: AUTO_RENDER_FOOTER,
          globals: {
            react: 'React',
            'react/jsx-runtime': 'ReactJSXRuntime',
            'react-native': 'ReactNative',
            'rill/reconciler': 'RillReconciler',
          },
          entryFileNames: outFileName,
        },
      },
    },
    resolve: {
      alias: {
        // Resolve to the rill package's SDK ESM export
        'rill/sdk': require.resolve('rill/sdk').replace('.js', '.mjs'),
      },
    },
    define: {
      'process.env.NODE_ENV': '"production"',
      __DEV__: 'false',
    },
    esbuild: {
      jsx: 'automatic',
    },
  };

  if (watch) {
    // Watch mode using Vite dev server in build watch mode
    console.log('Watching for changes...');

    const watcher = await viteBuild({
      ...viteConfig,
      build: {
        ...viteConfig.build,
        watch: {},
      },
    });

    // Handle exit signal
    process.on('SIGINT', async () => {
      console.log('\nStopping watch mode...');
      if (watcher && 'close' in watcher) {
        await (watcher as { close: () => Promise<void> }).close();
      }
      process.exit(0);
    });
  } else {
    // Single build
    try {
      await viteBuild(viteConfig);

      // Rename output file if needed (Vite adds format suffix)
      const generatedPath = path.join(outDir, outFileName.replace(/\.js$/, '.iife.js'));
      const targetPath = path.join(outDir, outFileName);
      if (fs.existsSync(generatedPath) && generatedPath !== targetPath) {
        fs.renameSync(generatedPath, targetPath);
      }

      // Output build info
      if (fs.existsSync(targetPath)) {
        const stats = fs.statSync(targetPath);
        const sizeKB = (stats.size / 1024).toFixed(2);
        const duration = Date.now() - startTime;

        console.log(`✅ Build successful!`);
        console.log(`   File: ${outfile}`);
        console.log(`   Size: ${sizeKB} KB`);
        console.log(`   Time: ${duration}ms`);

        // Output metafile (Vite doesn't have exact equivalent, create basic info)
        if (metafile) {
          const metaInfo = {
            inputs: { [entry]: { bytes: fs.statSync(entryPath).size } },
            outputs: { [outfile]: { bytes: stats.size } },
          };
          fs.writeFileSync(
            path.resolve(process.cwd(), metafile),
            JSON.stringify(metaInfo, null, 2)
          );
          console.log(`   Metafile: ${metafile}`);
        }
      }
    } catch (error) {
      // Enhanced error handling
      if (error instanceof Error) {
        if (error.message.includes('vite') || error.message.includes('rollup')) {
          console.error(`\n❌ Vite build error:`);
          console.error(`   ${error.message}`);
          console.error(`\n💡 Possible solutions:`);
          console.error(`   1. Check if vite is installed: npm list vite`);
          console.error(`   2. Reinstall dependencies: npm install`);
          console.error(`   3. Clean install: rm -rf node_modules && npm install`);
        } else if (error.message.includes('Cannot find module')) {
          console.error(`\n❌ Module not found:`);
          console.error(`   ${error.message}`);
          console.error(`\n💡 Tip:`);
          console.error(`   Check if the import path is correct`);
        } else {
          console.error(`\n❌ Build failed:`);
          console.error(`   ${error.message}`);
        }
      }

      throw error;
    }
  }
}

/**
 * Analyze bundle
 */
export async function analyze(bundlePath: string, options?: { whitelist?: string[]; failOnViolation?: boolean; treatEvalAsViolation?: boolean; treatDynamicNonLiteralAsViolation?: boolean }): Promise<void> {
  const fullPath = path.resolve(process.cwd(), bundlePath);
  if (!fs.existsSync(fullPath)) {
    throw new Error(`Bundle not found: ${fullPath}`);
  }

  const content = fs.readFileSync(fullPath, 'utf-8');
  const stats = fs.statSync(fullPath);
  const size = stats.size;
  const lines = content.split('\n').length;

  console.log('Bundle Analysis:');
  console.log(`  File: ${bundlePath}`);
  console.log(`  Size: ${(stats.size / 1024).toFixed(2)} KB`);
  console.log(`  Lines: ${content.split('\n').length}`);

  // Check for common issues
  if (content.includes('react-native')) {
    console.warn('  ⚠ Warning: Bundle contains "react-native" references');
  }

  if (content.includes('__DEV__') && !content.includes('__DEV__:false')) {
    console.warn('  ⚠ Warning: Bundle may contain development code');
  }

  // Oxc-only dependency scan (via adapter)
  const { analyzeModuleIDs } = await import('./oxcAdapter');
  const scan = await analyzeModuleIDs(content);
  const found = new Set<string>([
    ...scan.static,
    ...scan.dynamicLiteral,
    ...scan.details.map((d) => d.moduleId).filter(Boolean),
  ] as string[]);

  // If no deps were found by AST, do a safe dynamic import literal scan excluding comments/strings
  if (found.size === 0) {
    const ranges: Array<[number, number]> = [];
    const collect = (re: RegExp) => { let m: RegExpExecArray | null; while ((m = re.exec(content))) ranges.push([m.index, m.index + m[0].length]); };
    collect(/\/\*[\s\S]*?\*\//g); // block comments
    collect(/\/\/[^\n]*/g); // line comments
    collect(/'(?:\\.|[^'\\])*'/g); // single-quoted strings
    collect(/"(?:\\.|[^"\\])*"/g); // double-quoted strings
    collect(/`(?:\\.|[^`\\])*`/g); // template literals
    const inRange = (idx: number) => ranges.some(([s,e]) => idx >= s && idx < e);

    const addMatches = (re: RegExp) => {
      let m: RegExpExecArray | null;
      while ((m = re.exec(content))) if (!inRange(m.index) && m[2]) found.add(m[2]);
    };
    // dynamic import('x')
    addMatches(/import\((['"])(.*?)\1\)/g);
    // static import x from 'x'
    addMatches(/import\s+(?:[^'"]+?from\s+)?(['"])(.*?)\1/g);
    // bare import 'x'
    addMatches(/import\s+(['"])(.*?)\1\s*;?/g);
    // require('x')
    addMatches(/require\((['"])(.*?)\1\)/g);
  }

  // Try to find sourcemap (external or inline reference)
  let sourceMapPath: string | null = null;
  const sourceMapRefMatch = content.match(/\/\/# sourceMappingURL=(.*)$/m);
  if (sourceMapRefMatch && sourceMapRefMatch[1]) {
    const ref = sourceMapRefMatch[1].trim();
    if (!ref.startsWith('data:')) {
      const candidate = path.resolve(path.dirname(fullPath), ref);
      if (fs.existsSync(candidate)) sourceMapPath = candidate;
    }
  } else if (fs.existsSync(fullPath + '.map')) {
    sourceMapPath = fullPath + '.map';
  }
  type SourceMapSummary = { version?: unknown; sources?: unknown; file?: unknown } | { error: string } | null;
  let sourceMapSummary: SourceMapSummary = null;
  if (sourceMapPath) {
    try {
      const mapJson = JSON.parse(fs.readFileSync(sourceMapPath, 'utf-8')) as Record<string, unknown>;
      sourceMapSummary = { version: mapJson.version, sources: mapJson.sources, file: mapJson.file };
    } catch {
      sourceMapSummary = { error: 'failed_to_parse' };
    }
  }

  // JSON report (best-effort) with locations if available
  const report = {
    file: path.resolve(process.cwd(), bundlePath),
    size,
    lines,
    deps: Array.from(found),
    dynamicNonLiteral: scan.dynamicNonLiteral,
    evalCount: scan.evalCount,
    details: scan.details,
    sourceMap: sourceMapSummary,
  };
  const reportPath = path.resolve(process.cwd(), path.dirname(bundlePath), 'analyze.report.json');
  fs.writeFileSync(reportPath, JSON.stringify(report, null, 2));
  console.log('  Report:', path.relative(process.cwd(), reportPath));

  const whitelist = new Set(options?.whitelist ?? ['react', 'react-native', 'react/jsx-runtime', 'rill/reconciler']);
  const violations: string[] = Array.from(found).filter((m) => {
    if (whitelist.has(m)) return false;
    if (m.startsWith('./') || m.startsWith('../')) return false;
    // ignore URLs and virtual modules
    if (/^(data:|blob:|http:|https:|file:)/.test(m)) return false;
    if (m.includes('\0')) return false; // rollup virtual module marker
    return true;
  });
  // Violation conditions from flags
  if (options?.treatDynamicNonLiteralAsViolation && scan.dynamicNonLiteral > 0) {
    violations.push(`dynamic_import_non_literal:${scan.dynamicNonLiteral}`);
  }
  if (options?.treatEvalAsViolation && scan.evalCount > 0) {
    violations.push(`eval_calls:${scan.evalCount}`);
  }

  if (violations.length > 0) {
    const msg = `  ⚠ Warning: Found non-whitelisted modules: ${violations.join(', ')}`;
    if (options?.failOnViolation) {
      console.error(msg);
      throw new Error('Analyze failed: non-whitelisted modules detected');
    } else {
      console.warn(msg);
    }
  }
}
