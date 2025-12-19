/**
 * Rill CLI - Build
 *
 * Bun-based guest bundler
 */

import fs from 'fs';
import path from 'path';

/**
 * Build options
 */
export interface BuildOptions {
  /**
   * Enforce post-build dependency guard to prevent runtime requires in guest bundle.
   * If true (default), the analyzer will fail the build when detecting disallowed modules.
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

  // Callback registry - persist across re-executions
  if (!globalThis.__callbacks) {
    globalThis.__callbacks = new Map();
    globalThis.__callbackId = 0;
  }

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
        globalThis.__invokeCallback(message.fnId, message.args);
        break;
      case 'HOST_EVENT':
        globalThis.__handleHostEvent(message.eventName, message.payload);
        break;
      case 'CONFIG_UPDATE':
        if (globalThis.__config) {
          Object.assign(globalThis.__config, message.config);
        }
        break;
      case 'DESTROY':
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
 * Uses RillLet.render (from @rill/let global) for rendering
 */
const AUTO_RENDER_FOOTER = `
/* Auto-render */
(function() {
  if (typeof __sendToHost === 'function' && typeof globalThis.__RillGuest !== 'undefined') {
    try {
      var React = globalThis.React;
      if (!React) {
        console.error('[rill] React not found, cannot auto-render');
        return;
      }

      var RillLet = globalThis.RillLet;
      if (!RillLet || !RillLet.render) {
        console.error('[rill] RillLet not found, cannot auto-render');
        return;
      }

      var GuestExport = globalThis.__RillGuest;
      var Component = typeof GuestExport === 'function'
        ? GuestExport
        : (GuestExport.default || GuestExport);

      if (!Component || typeof Component !== 'function') {
        console.warn('[rill] No valid component found in guest');
        return;
      }

      var element = React.createElement(Component);
      console.log('[rill] Auto-rendering guest component');
      RillLet.render(element, __sendToHost);
    } catch (error) {
      console.error('[rill] Auto-render failed:', error);
    }
  }
})();
`;

/**
 * External modules and their global variable names
 */
const EXTERNALS: Record<string, string> = {
  react: 'React',
  'react/jsx-runtime': 'ReactJSXRuntime',
  'react/jsx-dev-runtime': 'ReactJSXDevRuntime',
  'react-native': 'ReactNative',
  '@rill/let': 'RillLet',
};

/**
 * Execute build using Bun.build
 */
export async function build(options: BuildOptions): Promise<void> {
  const startTime = Date.now();

  const { entry, outfile, minify, sourcemap, watch, metafile } = options;
  const strict = options.strict ?? true;

  // Validate entry file
  const entryPath = path.resolve(process.cwd(), entry);
  if (!fs.existsSync(entryPath)) {
    throw new Error(`Entry file not found: ${entryPath}`);
  }

  // Ensure output directory exists
  const outDir = path.dirname(path.resolve(process.cwd(), outfile));
  const outFileName = path.basename(outfile);
  if (!fs.existsSync(outDir)) {
    fs.mkdirSync(outDir, { recursive: true });
  }

  console.log(`Building ${entry}...`);

  if (watch) {
    console.log('Watch mode not yet implemented for Bun.build');
    console.log('Use: bun --watch build.ts for now');
    return;
  }

  // Build with Bun
  const result = await Bun.build({
    entrypoints: [entryPath],
    outdir: outDir,
    target: 'browser',
    format: 'iife',
    naming: outFileName.replace(/\.js$/, '') + '.[ext]',
    minify,
    sourcemap: sourcemap ? 'external' : 'none',
    external: Object.keys(EXTERNALS),
    define: {
      'process.env.NODE_ENV': '"production"',
      __DEV__: 'false',
    },
  });

  if (!result.success) {
    console.error('Build failed:');
    for (const log of result.logs) {
      console.error(log);
    }
    throw new Error('Bun build failed');
  }

  // Post-process: wrap in IIFE with globals mapping
  const targetPath = path.join(outDir, outFileName);
  let bundleCode = await Bun.file(result.outputs[0]!.path).text();

  // Transform external imports to global variable references
  // Bun marks externals as require() calls, we need to map them to globals
  for (const [mod, globalName] of Object.entries(EXTERNALS)) {
    // Replace require("module") with global variable
    const requirePattern = new RegExp(`require\\(["']${mod.replace('/', '\\/')}["']\\)`, 'g');
    bundleCode = bundleCode.replace(requirePattern, globalName);
  }

  // Bun IIFE format: (()=>{var exports={};...;return exports})();
  // We need to modify it to capture the default export into globalThis.__RillGuest
  //
  // Strategy: Replace the final })(); with code that exports to globalThis
  // The pattern looks like: var G={};m(G,{default:()=>p});...})();
  // where G is the exports object
  //
  // We inject code before the final })() to set globalThis.__RillGuest
  // by finding the exports object variable name

  // Find the exports variable pattern: var X={};m(X,{default:
  const exportsMatch = bundleCode.match(
    /var\s+([A-Za-z_$][A-Za-z0-9_$]*)={};[a-zA-Z_$]+\(\1,\{default:/
  );
  const exportsVar = exportsMatch ? exportsMatch[1] : null;

  let modifiedBundle = bundleCode;
  if (exportsVar) {
    // Inject code to capture exports before the IIFE closes
    // Replace final })(); with our capture code
    modifiedBundle = bundleCode.replace(
      /\}\)\(\);?\s*$/,
      `globalThis.__RillGuest = ${exportsVar}.default || ${exportsVar};\n})();`
    );
  } else {
    // Fallback: wrap the bundle code in a way that captures exports
    // This handles cases where the exports pattern isn't found
    modifiedBundle = `
var __rillExports = {};
var module = { exports: __rillExports };
var exports = __rillExports;
${bundleCode}
globalThis.__RillGuest = __rillExports.default || __rillExports;
`;
  }

  // Wrap with runtime inject and auto-render footer
  const wrappedCode = `/* Rill Guest Bundle - Generated by rill-cli */
${RUNTIME_INJECT}
${modifiedBundle}
${AUTO_RENDER_FOOTER}
/* End of Rill Guest Bundle */`;

  // Write final bundle
  await Bun.write(targetPath, wrappedCode);

  // Remove Bun's original output if different from target
  if (result.outputs[0]!.path !== targetPath) {
    fs.unlinkSync(result.outputs[0]!.path);
  }

  // Post-build strict dependency guard
  if (strict) {
    try {
      await analyze(targetPath, {
        whitelist: ['react', 'react-native', 'react/jsx-runtime', '@rill/let'],
        failOnViolation: true,
        treatEvalAsViolation: true,
        treatDynamicNonLiteralAsViolation: true,
      });
      console.log('   Strict guard: PASS');
    } catch (guardErr) {
      console.error('\n❌ Strict guard failed:');
      if (guardErr instanceof Error) console.error(`   ${guardErr.message}`);
      throw guardErr;
    }
  }

  // Validate bundle can be loaded with Function constructor
  try {
    const mockExports: Record<string, unknown> = {};
    const mockGlobals: Record<string, unknown> = {
      React: { createElement: () => ({}) },
      ReactJSXRuntime: { jsx: () => ({}), jsxs: () => ({}) },
      ReactJSXDevRuntime: { jsx: () => ({}), jsxs: () => ({}) },
      ReactNative: {},
      RillLet: { View: 'View', Text: 'Text', render: () => {} },
      module: { exports: mockExports },
      exports: mockExports,
    };
    // Mock require function that returns mock globals based on module name
    const mockRequire = (name: string) => {
      if (name === 'react') return mockGlobals.React;
      if (name === 'react/jsx-runtime') return mockGlobals.ReactJSXRuntime;
      if (name === 'react/jsx-dev-runtime') return mockGlobals.ReactJSXDevRuntime;
      if (name === 'react-native') return mockGlobals.ReactNative;
      if (name === '@rill/let') return mockGlobals.RillLet;
      return {};
    };
    const globalNames = ['require', ...Object.keys(mockGlobals)];
    const globalValues = [mockRequire, ...Object.values(mockGlobals)];
    new Function(...globalNames, wrappedCode)(...globalValues);
    console.log('   Syntax validation: PASS');
  } catch (validationErr) {
    console.error('\n❌ Bundle validation failed:');
    if (validationErr instanceof Error) {
      console.error('   Error:', validationErr.message);
    }
    throw validationErr;
  }

  // Output build info
  const stats = fs.statSync(targetPath);
  const sizeKB = (stats.size / 1024).toFixed(2);
  const duration = Date.now() - startTime;

  console.log(`✅ Build successful!`);
  console.log(`   File: ${outfile}`);
  console.log(`   Size: ${sizeKB} KB`);
  console.log(`   Time: ${duration}ms`);

  // Output metafile
  if (metafile) {
    const metaInfo = {
      inputs: { [entry]: { bytes: fs.statSync(entryPath).size } },
      outputs: { [outfile]: { bytes: stats.size } },
    };
    fs.writeFileSync(path.resolve(process.cwd(), metafile), JSON.stringify(metaInfo, null, 2));
    console.log(`   Metafile: ${metafile}`);
  }
}

/**
 * Analyze bundle for disallowed dependencies
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

  console.log('Bundle Analysis:');
  console.log(`  File: ${bundlePath}`);
  console.log(`  Size: ${(stats.size / 1024).toFixed(2)} KB`);
  console.log(`  Lines: ${content.split('\n').length}`);

  // Use oxc adapter for module analysis
  const { analyzeModuleIDs } = await import('./oxcAdapter');
  const scan = await analyzeModuleIDs(content);
  const found = new Set<string>([
    ...scan.static,
    ...scan.dynamicLiteral,
    ...scan.details.map((d) => d.moduleId).filter(Boolean),
  ] as string[]);

  const whitelist = new Set(
    options?.whitelist ?? ['react', 'react-native', 'react/jsx-runtime', '@rill/let']
  );

  const violations: string[] = Array.from(found).filter((m) => {
    if (whitelist.has(m)) return false;
    if (m.startsWith('./') || m.startsWith('../')) return false;
    if (/^(data:|blob:|http:|https:|file:)/.test(m)) return false;
    if (m.includes('\0')) return false;
    return true;
  });

  if (options?.treatDynamicNonLiteralAsViolation && scan.dynamicNonLiteral > 0) {
    violations.push(`dynamic_import_non_literal:${scan.dynamicNonLiteral}`);
  }
  if (options?.treatEvalAsViolation && scan.evalCount > 0) {
    violations.push(`eval_calls:${scan.evalCount}`);
  }

  if (violations.length > 0) {
    const msg = `Found non-whitelisted modules: ${violations.join(', ')}`;
    if (options?.failOnViolation) {
      throw new Error(msg);
    } else {
      console.warn(`  ⚠ Warning: ${msg}`);
    }
  }
}
