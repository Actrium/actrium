/**
 * WASM Sandbox E2E Test Runner
 *
 * Copies WASM files, starts server, and runs Playwright tests
 */

import { spawn } from 'child_process';
import { copyFileSync, existsSync, mkdirSync } from 'fs';
import { join } from 'path';

const ROOT = join(import.meta.dir, '../..');
const E2E_DIR = import.meta.dir;
const DIST_DIR = join(E2E_DIR, 'dist');
const WASM_SRC = join(ROOT, 'src/sandbox/wasm');

async function copyWASMFiles() {
  console.log('Copying WASM files...');

  if (!existsSync(DIST_DIR)) {
    mkdirSync(DIST_DIR, { recursive: true });
  }

  const files = ['quickjs_sandbox.js', 'quickjs_sandbox.wasm'];
  for (const file of files) {
    const src = join(WASM_SRC, file);
    const dest = join(DIST_DIR, file);
    if (existsSync(src)) {
      copyFileSync(src, dest);
      console.log(`  ${file} -> dist/`);
    } else {
      console.error(`  Missing: ${src}`);
      console.error('  Run: cd native/quickjs && ./build-wasm.sh release');
      process.exit(1);
    }
  }
}

async function main() {
  // Copy WASM files
  await copyWASMFiles();

  // Start Bun server
  const server = Bun.serve({
    port: 0,
    hostname: '127.0.0.1',

    async fetch(req) {
      const url = new URL(req.url);
      let pathname = url.pathname;

      if (pathname === '/') {
        pathname = '/index.html';
      }

      // Serve from dist or e2e directory
      let filePath: string;
      if (pathname.startsWith('/dist/')) {
        filePath = join(DIST_DIR, pathname.slice(6));
      } else {
        filePath = join(E2E_DIR, pathname);
      }

      const file = Bun.file(filePath);
      if (await file.exists()) {
        const contentType = getContentType(pathname);
        return new Response(file, {
          headers: {
            'Content-Type': contentType,
            // Required for WASM
            'Cross-Origin-Opener-Policy': 'same-origin',
            'Cross-Origin-Embedder-Policy': 'require-corp',
          },
        });
      }

      return new Response('Not Found', { status: 404 });
    },
  });

  const port = server.port;
  console.log(`Server started on http://127.0.0.1:${port}`);

  // Run Playwright
  const playwright = spawn(
    'npx',
    ['playwright', 'test', '--config', 'tests/e2e-wasm-sandbox/playwright.config.ts'],
    {
      stdio: 'inherit',
      cwd: ROOT,
      env: {
        ...process.env,
        TEST_PORT: String(port),
      },
    }
  );

  playwright.on('close', async (code) => {
    server.stop();
    process.exit(code ?? 0);
  });
}

function getContentType(pathname: string): string {
  if (pathname.endsWith('.html')) return 'text/html';
  if (pathname.endsWith('.js')) return 'application/javascript';
  if (pathname.endsWith('.wasm')) return 'application/wasm';
  if (pathname.endsWith('.css')) return 'text/css';
  if (pathname.endsWith('.json')) return 'application/json';
  return 'application/octet-stream';
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
