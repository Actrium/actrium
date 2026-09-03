import { readFileSync } from 'node:fs';

import { describe, expect, it } from 'vitest';

// `actor.sw.js` is a standalone classic service-worker script with no module
// system, so this test extracts the real mapping function from the shipped
// bytes between the `@host-error-map` markers and evaluates it directly —
// the tested code is the code the SW runs, not a copy.
function loadHostErrorMap(sourceUrl) {
  const source = readFileSync(sourceUrl, 'utf8');
  const begin = source.indexOf('// @host-error-map begin');
  const end = source.indexOf('// @host-error-map end');
  expect(begin, 'actor.sw.js must keep the @host-error-map begin marker').toBeGreaterThan(-1);
  expect(end, 'actor.sw.js must keep the @host-error-map end marker').toBeGreaterThan(begin);
  const body = source.slice(begin, end);
  return new Function(`${body}\nreturn mapHostErrorToActr;`)();
}

function hostErrorMapBody(sourceUrl) {
  const source = readFileSync(sourceUrl, 'utf8');
  return source.slice(
    source.indexOf('// @host-error-map begin'),
    source.indexOf('// @host-error-map end')
  );
}

const swSource = new URL('./actor.sw.js', import.meta.url);
// The CLI embeds a copy of the runtime; both must carry the same mapping.
const cliCopySource = new URL(
  '../../../../../crates/actr-cli/assets/web-runtime/actor.sw.js',
  import.meta.url
);

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
    const swFn = hostErrorMapBody(swSource);
    const cliFn = hostErrorMapBody(cliCopySource);
    // Only the marker comment differs between the two copies; the function
    // body must be identical so neither runtime diverges.
    const stripMarkerComment = (text) =>
      text
        .split('\n')
        .filter((line) => !line.startsWith('//'))
        .join('\n');
    expect(stripMarkerComment(cliFn)).toBe(stripMarkerComment(swFn));
  });
});
