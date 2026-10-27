import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

// Exercise the classic worker's actual comparison code, including after its
// private import material has been discarded following Host registration.
const source = readFileSync(new URL('./actor.sw.js', import.meta.url), 'utf8');
const begin = source.indexOf('function canonicalRuntimeConfig(');
const end = source.indexOf('function validateRuntimeConfig(', begin);
const compare = new Function(`${source.slice(begin, end)}\nreturn runtimeConfigsEqual;`)();
const config = { realm: 'browser@example.com', issuer_endpoint: 'https://provider.example/issuer' };
const imported = (id) => ({ ...config, realm_client_key: { key_id: id } });

describe('Realm key import across browser clients', () => {
  it('allows later tabs to restore an imported key without resending its secret', () => {
    expect(compare(config, imported('device-a'))).toBe(true);
    expect(compare(imported('device-a'), config, 'device-a')).toBe(true);
  });

  it('refuses switching the key of an already configured worker', () => {
    expect(compare(imported('device-b'), config, 'device-a')).toBe(false);
    expect(compare(imported('device-b'), config, null)).toBe(false);
    expect(compare({ ...config, realm: 'other@example.com' }, config, 'device-a')).toBe(false);
  });
});
