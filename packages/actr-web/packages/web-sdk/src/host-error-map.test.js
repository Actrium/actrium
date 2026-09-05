import { existsSync, readFileSync } from 'node:fs';

import { describe, expect, it } from 'vitest';

// `actor.sw.js` is a standalone classic service-worker script with no module
// system, so this test extracts the real mapping function from the shipped
// bytes between the `@host-error-map` markers and evaluates it directly —
// the tested code is the code the SW runs, not a copy.
function loadHostErrorMap(sourceUrl) {
  const source = readFileSync(sourceUrl, 'utf8');
  const begin = source.indexOf('// @host-error-map begin');
  // Search for the end marker only at or after the begin marker so a missing
  // marker cannot produce a nonsensical negative range.
  const end = source.indexOf('// @host-error-map end', begin + 1);
  expect(begin, 'actor.sw.js must keep the @host-error-map begin marker').toBeGreaterThan(-1);
  expect(
    end,
    'actor.sw.js must keep the @host-error-map end marker after the begin marker'
  ).toBeGreaterThan(begin);
  const body = source.slice(begin, end);
  return new Function(`${body}\nreturn mapHostErrorToActr;`)();
}

// The two runtime copies intentionally carry different marker-comment
// blocks (each documents its own role); everything from the function
// declaration (located directly, independent of the marker comments) to the
// end of the file's mapping section must be byte-identical, comments
// included. The region ends at the next top-level declaration so trailing
// whitespace differences cannot mask divergence either.
function hostErrorMapFunctionRegion(sourceUrl) {
  const source = readFileSync(sourceUrl, 'utf8');
  const start = source.indexOf('function mapHostErrorToActr');
  const endMarker = source.indexOf('// @host-error-map end');
  const nextDeclaration = source.indexOf('\nfunction ', start + 1);
  const end = nextDeclaration === -1 || nextDeclaration > endMarker ? endMarker : nextDeclaration;
  return source.slice(start, end);
}

const swSource = new URL('./actor.sw.js', import.meta.url);
// The CLI embeds a copy of the runtime; both must carry the same mapping.
const cliCopySource = new URL(
  '../../../../../crates/actr-cli/assets/web-runtime/actor.sw.js',
  import.meta.url
);
// The package publishes the built worker; CI builds before testing, so the
// published dist bytes must exist and match the sources (a stale or
// diverging build cannot ship an old error mapping behind a green
// source-level test). When dist is absent (pre-build local run), the
// source-level assertions below still gate the mapping.
const swDistSource = new URL('../dist/actor.sw.js', import.meta.url);
const swDistExists = existsSync(swDistSource);

describe('mapHostErrorToActr (actor.sw.js)', () => {
  const mapHostErrorToActr = loadHostErrorMap(swSource);

  it('reconstructs delivery-uncertain with the exact operation id and cause', () => {
    const mapped = mapHostErrorToActr({
      actrErrorTag: 'delivery-uncertain',
      actrMessage: 'write may have entered transport',
      actrOperationId: 'stream-1#4',
      message: 'delivery-uncertain: write may have entered transport',
    });
    expect(mapped).toEqual({
      'delivery-uncertain': {
        'operation-id': 'stream-1#4',
        cause: 'write may have entered transport',
      },
    });
  });

  it('keeps operation-id and cause independent when either contains separators', () => {
    const mapped = mapHostErrorToActr({
      actrErrorTag: 'delivery-uncertain',
      actrMessage: 'fragmented send interrupted: channel closed',
      actrOperationId: 'op:42#7',
      message: 'delivery-uncertain: fragmented send interrupted: channel closed',
    });
    expect(mapped['delivery-uncertain']['operation-id']).toBe('op:42#7');
    expect(mapped['delivery-uncertain'].cause).toBe('fragmented send interrupted: channel closed');
  });

  it('falls back to an empty operation id and a message-derived cause', () => {
    const mapped = mapHostErrorToActr({
      actrErrorTag: 'delivery-uncertain',
      message: 'delivery-uncertain: partial write',
    });
    expect(mapped['delivery-uncertain']['operation-id']).toBe('');
    expect(mapped['delivery-uncertain'].cause).toBe('delivery-uncertain: partial write');
  });

  it('maps other tagged host errors unchanged', () => {
    expect(mapHostErrorToActr({ actrErrorTag: 'timed-out' })).toBe('timed-out');
    expect(mapHostErrorToActr({ actrErrorTag: 'not-found', actrMessage: 'no actor' })).toEqual({
      'not-found': 'no actor',
    });
  });

  it('keeps the CLI-embedded runtime copy byte-identical to the web-sdk mapping', () => {
    const swFn = hostErrorMapFunctionRegion(swSource);
    const cliFn = hostErrorMapFunctionRegion(cliCopySource);
    // Only the marker comment block above the function differs between the
    // two copies; the function region itself — comments included — must be
    // byte-identical so neither runtime diverges.
    expect(cliFn).toBe(swFn);
  });

  it('publishes the same mapping through the built dist service worker', () => {
    if (!swDistExists) {
      // Source-level coverage still ran above. Under CI the SDK is always
      // built first, so a missing dist there means the build failed to copy
      // the worker — fail instead of silently skipping the dist gate.
      if (process.env.CI) {
        throw new Error('CI must build the web-sdk before running this suite');
      }
      return;
    }
    const distFn = hostErrorMapFunctionRegion(swDistSource);
    const srcFn = hostErrorMapFunctionRegion(swSource);
    expect(distFn).toBe(srcFn);
    const mapped = loadHostErrorMap(swDistSource)({
      actrErrorTag: 'delivery-uncertain',
      actrMessage: 'write may have entered transport',
      actrOperationId: 'stream-1#4',
      message: 'delivery-uncertain: write may have entered transport',
    });
    expect(mapped).toEqual({
      'delivery-uncertain': {
        'operation-id': 'stream-1#4',
        cause: 'write may have entered transport',
      },
    });
  });
});
