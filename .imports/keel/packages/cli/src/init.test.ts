import { afterAll, beforeAll, describe, expect, it } from 'bun:test';
import fs from 'fs';
import path from 'path';

const TMP_ROOT = path.join(process.cwd(), 'tmp_rovodev_cli_init');

describe('CLI init', () => {
  beforeAll(() => {
    fs.mkdirSync(TMP_ROOT, { recursive: true });
  });

  afterAll(() => {
    // best-effort cleanup
    try {
      fs.rmSync(TMP_ROOT, { recursive: true, force: true });
    } catch {}
  });

  it('should scaffold a minimal guest project', async () => {
    const cwd = process.cwd();
    process.chdir(TMP_ROOT);
    const argv = process.argv;
    process.argv = ['node', 'rill', 'init', 'my-rill-guest'];

    // dynamic import to execute commander program
    await import('./index');

    // wait for async action to complete
    await new Promise((r) => setTimeout(r, 20));

    // restore env
    process.argv = argv;
    process.chdir(cwd);

    const base = path.join(TMP_ROOT, 'my-rill-guest');
    expect(fs.existsSync(path.join(base, 'package.json'))).toBe(true);
    expect(fs.existsSync(path.join(base, 'tsconfig.json'))).toBe(true);
    expect(fs.existsSync(path.join(base, 'src', 'guest.tsx'))).toBe(true);

    const pkg = JSON.parse(fs.readFileSync(path.join(base, 'package.json'), 'utf-8'));
    expect(pkg.scripts.build).toBeDefined();
  });
});
