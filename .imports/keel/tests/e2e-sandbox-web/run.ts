// e2e/run.ts - Build with Bun, start server, then run Playwright

import { spawn } from 'child_process';
import { join } from 'path';

const ROOT = join(import.meta.dir, '../..');
const E2E_DIR = import.meta.dir;
const DIST_DIR = join(E2E_DIR, 'dist');

async function build() {
  console.log('Building with Bun...');

  // Build main entry
  const mainResult = await Bun.build({
    entrypoints: [join(ROOT, 'src/sandbox-web/index.ts')],
    outdir: DIST_DIR,
    target: 'browser',
    format: 'esm',
    naming: '[name].js',
  });

  if (!mainResult.success) {
    console.error('Main build failed:', mainResult.logs);
    process.exit(1);
  }

  // Build worker separately
  const workerResult = await Bun.build({
    entrypoints: [join(ROOT, 'src/sandbox-web/worker.ts')],
    outdir: DIST_DIR,
    target: 'browser',
    format: 'esm',
    naming: 'worker.js',
  });

  if (!workerResult.success) {
    console.error('Worker build failed:', workerResult.logs);
    process.exit(1);
  }

  console.log('Build complete.');
}

async function main() {
  // Build first
  await build();

  // Start Bun server
  const server = Bun.serve({
    port: 0, // Auto-assign port
    hostname: '127.0.0.1',

    async fetch(req) {
      const url = new URL(req.url);
      let pathname = url.pathname;

      // Default to index.html
      if (pathname === '/') {
        pathname = '/index.html';
      }

      // Serve from e2e directory or dist
      let filePath: string;
      if (pathname.startsWith('/dist/')) {
        filePath = join(DIST_DIR, pathname.slice(6));
      } else {
        filePath = join(E2E_DIR, pathname);
      }

      const file = Bun.file(filePath);
      if (await file.exists()) {
        return new Response(file, {
          headers: {
            'Content-Type': getContentType(pathname),
          },
        });
      }

      return new Response('Not Found', { status: 404 });
    },
  });

  const port = server.port;
  console.log(`Bun server started on port ${port}`);

  // Run Playwright with the dynamic port
  const playwright = spawn(
    'npx',
    ['playwright', 'test', '--config', 'tests/e2e-sandbox-web/playwright.config.ts'],
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
  if (pathname.endsWith('.ts')) return 'application/javascript';
  if (pathname.endsWith('.css')) return 'text/css';
  if (pathname.endsWith('.json')) return 'application/json';
  return 'application/octet-stream';
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
