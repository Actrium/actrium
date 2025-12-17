/**
 * Rill CLI - Build
 *
 * Vite-based guest bundler (replaced esbuild)
 */

import fs from 'fs';
import path from 'path';
import type { InlineConfig } from 'vite';
import { build as viteBuild } from 'vite';

/**
 * Build options
 */
export interface BuildOptions {
  /**
   * Enforce post-build dependency guard to prevent runtime requires in guest bundle.
   * If true (default), the analyzer will fail the build when detecting disallowed modules like '@rill/core/sdk',
   * dynamic non-literal imports, or eval usage (configurable).
   */
  strict?: boolean;
  /** strict peer versions */
  strictPeerVersions?: boolean;
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

  // DEBUG: Test if console is working
  if (typeof console !== 'undefined' && typeof console.log === 'function') {
    console.log('[rill:bundle] RUNTIME_INJECT starting, console.log works!');
  }
  if (typeof __console_log === 'function') {
    __console_log('[rill:bundle] RUNTIME_INJECT: __console_log direct call works!');
  }

  // Callback registry - persist across re-executions
  if (!globalThis.__callbacks) {
    globalThis.__callbacks = new Map();
    globalThis.__callbackId = 0;
  }
  var __callbacks = globalThis.__callbacks;
  var __callbackIdCounter = globalThis.__callbackId;

  // Register callback
  globalThis.__registerCallback = function(fn) {
    var id = 'fn_' + (++globalThis.__callbackId);
    globalThis.__callbacks.set(id, fn);
    return id;
  };

  // Invoke callback
  globalThis.__invokeCallback = function(fnId, args) {
    var fn = globalThis.__callbacks.get(fnId);
    if (fn) {
      try {
        return fn.apply(null, args || []);
      } catch (e) {
        console.error('[rill] Callback error:', e);
      }
    } else {
      console.warn('[rill] Callback not found:', fnId);
      console.warn('[rill] Available callbacks:', Array.from(globalThis.__callbacks.keys()).join(', '));
    }
  };

  // Remove callback
  globalThis.__removeCallback = function(fnId) {
    globalThis.__callbacks.delete(fnId);
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
        // Do not clear callbacks, as the registry is now global across engine instances
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
/* Auto-render - uses globalThis.__RillGuest for JSC sandbox compatibility */
(function() {
  console.log('[rill:auto-render] Starting AUTO_RENDER_FOOTER...');
  console.log('[rill:auto-render] typeof __sendToHost:', typeof __sendToHost);
  console.log('[rill:auto-render] typeof globalThis.__RillGuest:', typeof globalThis.__RillGuest);

  if (typeof __sendToHost === 'function' && typeof globalThis.__RillGuest !== 'undefined') {
    console.log('[rill:auto-render] Conditions passed, entering try block');
    try {
      // Use globalThis.React directly (injected by REACT_SHIM)
      // Don't use require('react') as it goes through JSI bridge and loses functions
      var React = globalThis.React;
      console.log('[rill:auto-render] React:', React ? 'found' : 'NOT found');
      console.log('[rill:auto-render] React.createElement:', typeof React.createElement);

      if (!React) {
        console.error('[rill] React not found, cannot auto-render');
        return;
      }

      // Import render function from reconciler
      // In Engine's mock runtime, this will be available via require polyfill
      var reconciler = typeof require === 'function' ? require('rill/reconciler') : null;
      console.log('[rill:auto-render] reconciler:', reconciler ? 'found' : 'NOT found');
      console.log('[rill:auto-render] reconciler.render:', reconciler && reconciler.render ? 'found' : 'NOT found');

      if (!reconciler || !reconciler.render) {
        console.error('[rill] Reconciler not found, cannot auto-render');
        return;
      }

      // Get the default export (the main component)
      // IIFE format sets globalThis.__RillGuest, may return the component directly or as .default
      var GuestExport = globalThis.__RillGuest;
      var Component = typeof GuestExport === 'function'
        ? GuestExport
        : (GuestExport.default || GuestExport);
      console.log('[rill:auto-render] Component type:', typeof Component);

      if (!Component || typeof Component !== 'function') {
        console.warn('[rill] No valid component found in guest');
        return;
      }

      // Create React element and render via reconciler
      var element = React.createElement(Component);
      console.log('[rill:auto-render] element:', element ? 'created' : 'null');

      console.log('[rill] Auto-rendering guest component');
      console.log('[rill:auto-render] Calling reconciler.render with sendToHost type:', typeof __sendToHost);
      reconciler.render(element, __sendToHost);
      console.log('[rill:auto-render] reconciler.render returned');

      // 🔴 TRACK: Read after commit phase completes
      setTimeout(function() {
        console.log('[Guest] === After Commit ===');
        console.log('[Guest] Total ops:', typeof globalThis !== 'undefined' ? globalThis.__TOTAL_OPS : 0);
        console.log('[Guest] Op types:', typeof globalThis !== 'undefined' ? JSON.stringify(globalThis.__OP_COUNTS) : '{}');
        console.log('[Guest] TouchableOpacity created:', typeof globalThis !== 'undefined' ? globalThis.__TOUCHABLE_CREATE_COUNT : 0);
        console.log('[Guest] Last TouchableOpacity fnId:', typeof globalThis !== 'undefined' ? globalThis.__LAST_TOUCHABLE_FNID : 'none');
      }, 100);

    } catch (error) {
      // Better error logging - extract message and stack if available
      var errMsg = error && error.message ? error.message : String(error);
      var errStack = error && error.stack ? error.stack : '(no stack)';
      var errName = error && error.name ? error.name : '(no name)';
      console.error('[rill] Auto-render failed:', errName, '-', errMsg);
      console.error('[rill] Error stack:', errStack);
    }
  }
})();

/* End of Rill Guest Bundle */`;

/**
 * Execute build using Vite
 */
export async function build(options: BuildOptions): Promise<void> {
  const startTime = Date.now();

  const { entry, outfile, minify, sourcemap, watch, metafile } = options;

  const strict = options.strict ?? true;
  const strictPeerVersions = options.strictPeerVersions ?? false;

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

  // Vite configuration for guest build
  const viteConfig: InlineConfig = {
    configFile: false,
    root: process.cwd(),
    mode: 'production',
    logLevel: 'warn',
    build: {
      lib: {
        entry: entryPath,
        name: '__RillGuest',
        formats: ['iife'],
        fileName: () => outFileName.replace(/\.js$/, ''),
      },
      outDir,
      emptyOutDir: false,
      sourcemap,
      minify: minify ? 'esbuild' : false,
      target: 'es2020',
      rollupOptions: {
        external: [
          'react',
          'react/jsx-runtime',
          'react/jsx-dev-runtime',
          'react-native',
          '@rill/core',
          '@rill/core/sdk',
        ],
        output: {
          format: 'iife',
          name: '__RillGuest',
          banner: `/* Rill Guest Bundle - Generated by rill-cli */\n${RUNTIME_INJECT}`,
          footer: AUTO_RENDER_FOOTER,
          globals: {
            react: 'React',
            'react/jsx-runtime': 'ReactJSXRuntime',
            'react/jsx-dev-runtime': 'ReactJSXDevRuntime',
            'react-native': 'ReactNative',
            '@rill/core': 'RillCore',
            '@rill/core/sdk': 'RillCore', // Map to same global as @rill/core
          },
          entryFileNames: outFileName,
        },
        plugins: [
          // Transform var __RillGuest to globalThis.__RillGuest for JSC sandbox compatibility
          // In JSC's evaluateScript, var declarations don't become globalThis properties
          {
            name: 'rill-globalThis-fix',
            generateBundle(_options, bundle) {
              for (const fileName in bundle) {
                const item = bundle[fileName];
                if (item && item.type === 'chunk') {
                  const chunk = item;
                  // Replace var __RillGuest= with globalThis.__RillGuest=
                  chunk.code = chunk.code.replace(
                    /^var __RillGuest\s*=/,
                    'globalThis.__RillGuest='
                  );
                }
              }
            },
          },
          // NOTE: Disabled transformation - use global variable injection at runtime instead
          // See FunctionQuickJSProvider for global variable setup
          /*
          {
            name: 'rill-transform-globals-to-require',
            generateBundle(_options, bundle) {
              for (const fileName in bundle) {
                const item = bundle[fileName];
                if (item && item.type === 'chunk') {
                  const chunk = item; // Type narrowing
                  // Transform IIFE that expects globals to use require() instead
                  const moduleMap: Record<string, string> = {
                    'ReactJSXRuntime': 'react/jsx-runtime',
                    'React': 'react',
                    'ReactNative': 'react-native',
                    'RillReconciler': 'rill/reconciler',
                    'RillSDK': 'rill/sdk',
                  };

                  // Save original bundle before transformation for debugging
                  const originalPath = fileName.replace(/\.js$/, '.original.js');
                  fs.writeFileSync(originalPath, chunk.code, 'utf-8');
                  console.log(`📝 Original bundle saved to: ${originalPath}`);

                  // Match the IIFE signature with parameters: (function(t,i,o){
                  const signatureMatch = chunk.code.match(/var __RillGuest=\(function\(([^)]*)\)\{/);
                  // Match the IIFE invocation with arguments: })(ReactJSXRuntime,React,RillSDK);
                  const invokeMatch = chunk.code.match(/\)\(([\w,\s]+)\);(?=\(function\(\)\{|\s*\/\*\s*Auto-render)/);

                  if (signatureMatch && invokeMatch && signatureMatch[1] && invokeMatch[1]) {
                    // Get the minified parameter names (t, i, o, ...)
                    const minifiedParams = signatureMatch[1].split(',').map((p: string) => p.trim()).filter(Boolean);
                    // Get the global module names (ReactJSXRuntime, React, RillSDK, ...)
                    const globalParams = invokeMatch[1].split(',').map((p: string) => p.trim()).filter(Boolean);

                    // Generate require statements using minified parameter names
                    // This preserves the variable names that the minified code expects
                    const requireStatements = minifiedParams
                      .map((minifiedName: string, index: number) => {
                        const globalName = globalParams[index];
                        const moduleName = globalName ? moduleMap[globalName] : null;
                        return moduleName ? `var ${minifiedName}=require('${moduleName}');` : null;
                      })
                      .filter(Boolean)
                      .join('\n');

                    // IMPORTANT: Must remove "use strict" BEFORE adding require statements
                    // Otherwise "use strict" won't be the first statement

                    // First, replace IIFE signature and remove the "use strict" that immediately follows
                    // Match: (function(t,i,o){"use strict"; → (function(){
                    // Note: "use strict" might not have any whitespace after the semicolon
                    chunk.code = chunk.code.replace(
                      /var __RillGuest=\(function\([^)]*\)\{("use strict";|'use strict';)/,
                      `var __RillGuest=(function(){\n${requireStatements}\n`
                    );

                    // Note: FunctionContext will add "use strict" at the outermost level

                    // Remove the IIFE invocation arguments: })(React,RillSDK,...); → })();
                    chunk.code = chunk.code.replace(
                      /\)\(([\w,\s]+)\);(?=\(function\(\)\{|\s*\/\*\s*Auto-render)/,
                      '();'
                    );

                    // Validate syntax using Function constructor (more strict than oxc-parser)
                    // This simulates exactly how FunctionQuickJSProvider will execute the code
                    try {
                      // Test without "use strict" first
                      new Function('require', chunk.code);
                      // Test with "use strict" as FunctionContext will add it
                      new Function('require', `"use strict"; ${chunk.code}`);
                      console.log(`✅ Bundle ${fileName} passed Function constructor validation`);
                    } catch (parseError) {
                      const err = parseError as Error;
                      // Save the problematic bundle for debugging
                      const debugPath = fileName.replace(/\.js$/, '.debug.js');
                      fs.writeFileSync(debugPath, chunk.code, 'utf-8');
                      console.error(`❌ Syntax error in transformed bundle ${fileName}:`);
                      console.error('Error:', err.message);
                      console.error(`Debug bundle saved to: ${debugPath}`);
                      console.error('First 500 chars:', chunk.code.substring(0, 500));
                      console.error('Last 200 chars:', chunk.code.substring(chunk.code.length - 200));
                      throw new Error(`Bundle transformation produced invalid syntax: ${err.message}`);
                    }
                  }
                }
              }
            },
          },*/
        ],
      },
    },
    resolve: {
      alias: {
        // @rill/core publishes TypeScript source, Vite can handle .ts files directly
        '@rill/core/sdk': require.resolve('@rill/core/sdk'),
        '@rill/core': require.resolve('@rill/core'),
      },
    },
    define: {
      'process.env.NODE_ENV': '"production"',
      __DEV__: 'false',
    },
    esbuild: {
      jsx: 'automatic',
      jsxDev: false, // Force production JSX runtime
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

      // Peer version matrix check
      try {
        // eslint-disable-next-line @typescript-eslint/no-var-requires
        const reactPkg = require(
          require.resolve('react/package.json', { paths: [process.cwd()] })
        ) as { version?: string };
        let reconcilerPkg: { version?: string } | undefined;
        try {
          // eslint-disable-next-line @typescript-eslint/no-var-requires
          reconcilerPkg = require(
            require.resolve('react-reconciler/package.json', { paths: [process.cwd()] })
          ) as { version?: string };
        } catch {
          // Reconciler package not found, skip version check
        }
        if (reactPkg && reconcilerPkg) {
          const rv = String(reactPkg.version || '');
          const tv = String(reconcilerPkg.version || '');
          const mapping: Array<{ r: RegExp; allowed: RegExp; recommend: string }> = [
            { r: /^18\./, allowed: /^(0\.(29|30|31))\./, recommend: 'react-reconciler ^0.31' },
            { r: /^19\.0\./, allowed: /^0\.32\./, recommend: 'react-reconciler ^0.32' },
            { r: /^19\.(2|3|4|5)\./, allowed: /^0\.33\./, recommend: 'react-reconciler ^0.33' },
          ];
          const m = mapping.find((x) => x.r.test(rv));
          if (m && !m.allowed.test(tv)) {
            const msg = `[rill] React ${rv} with react-reconciler ${tv}. Recommended: ${m.recommend}`;
            if (strictPeerVersions) throw new Error(msg);
            else console.warn(msg);
          }
        }
      } catch (e) {
        if (strictPeerVersions) throw e;
        // ignore if not resolvable
      }

      // Post-build strict dependency guard
      if (strict) {
        try {
          await analyze(path.join(outDir, outFileName), {
            whitelist: [
              'react',
              'react-native',
              'react/jsx-runtime',
              '@rill/core',
              '@rill/core/sdk',
              'rill/reconciler',
            ],
            failOnViolation: true,
            treatEvalAsViolation: true,
            treatDynamicNonLiteralAsViolation: true,
          });
          console.log('   Strict guard: PASS (no disallowed runtime deps)');
        } catch (guardErr) {
          console.error('\n❌ Strict guard failed:');
          if (guardErr instanceof Error) console.error(`   ${guardErr.message}`);
          console.error('   Hint: Remove runtime imports like "rill/sdk" from the bundle.');
          console.error('   The SDK must be compiled-time inlined.');
          throw guardErr;
        }
      }

      // Validate bundle can be loaded with Function constructor
      try {
        const bundleCode = fs.readFileSync(targetPath, 'utf-8');

        // Mock the global variables that will be injected at runtime
        const mockGlobals = {
          ReactJSXRuntime: {},
          ReactJSXDevRuntime: {},
          React: {},
          RillCore: {}, // New unified @rill/core global
          ReactNative: {},
        };

        const globalNames = Object.keys(mockGlobals);
        const globalValues = Object.values(mockGlobals);

        // Test with Function constructor (simulates FunctionQuickJSProvider)
        new Function(...globalNames, bundleCode)(...globalValues);

        console.log('   Syntax validation: PASS');
      } catch (validationErr) {
        console.error('\n❌ Bundle validation failed:');
        if (validationErr instanceof Error) {
          console.error('   Error:', validationErr.message);
          console.error('   The bundle cannot be loaded by Function constructor');
        }
        throw validationErr;
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
export async function analyze(
  bundlePath: string,
  options?: {
    whitelist?: string[];
    failOnViolation?: boolean;
    treatEvalAsViolation?: boolean;
    treatDynamicNonLiteralAsViolation?: boolean;
  }
): Promise<void> {
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
    const collect = (re: RegExp) => {
      let m: RegExpExecArray | null;
      while ((m = re.exec(content))) ranges.push([m.index, m.index + m[0].length]);
    };
    collect(/\/\*[\s\S]*?\*\//g); // block comments
    collect(/\/\/[^\n]*/g); // line comments
    collect(/'(?:\\.|[^'\\])*'/g); // single-quoted strings
    collect(/"(?:\\.|[^"\\])*"/g); // double-quoted strings
    collect(/`(?:\\.|[^`\\])*`/g); // template literals
    const inRange = (idx: number) => ranges.some(([s, e]) => idx >= s && idx < e);

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
  if (sourceMapRefMatch?.[1]) {
    const ref = sourceMapRefMatch[1].trim();
    if (!ref.startsWith('data:')) {
      const candidate = path.resolve(path.dirname(fullPath), ref);
      if (fs.existsSync(candidate)) sourceMapPath = candidate;
    }
  } else if (fs.existsSync(`${fullPath}.map`)) {
    sourceMapPath = `${fullPath}.map`;
  }
  type SourceMapSummary =
    | { version?: unknown; sources?: unknown; file?: unknown }
    | { error: string }
    | null;
  let sourceMapSummary: SourceMapSummary = null;
  if (sourceMapPath) {
    try {
      const mapJson = JSON.parse(fs.readFileSync(sourceMapPath, 'utf-8')) as {
        version?: unknown;
        sources?: unknown;
        file?: unknown;
      };
      sourceMapSummary = {
        version: mapJson.version,
        sources: mapJson.sources,
        file: mapJson.file,
      };
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

  const whitelist = new Set(
    options?.whitelist ?? [
      'react',
      'react-native',
      'react/jsx-runtime',
      '@rill/core',
      '@rill/core/sdk',
      'rill/reconciler',
    ]
  );
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
