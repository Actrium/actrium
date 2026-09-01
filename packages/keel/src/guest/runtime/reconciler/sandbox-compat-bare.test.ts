/**
 * Bare-sandbox regression guard for the sandbox-safe helpers (issue #158).
 *
 * `sandbox-compat.ts`'s header claims two things that only a bare environment can
 * check: that every helper works with no `TextEncoder` / `TextDecoder` /
 * `performance` / `btoa` / `atob`, and that the checks happen per CALL so nothing
 * is captured at module-eval time. The file also says "tests can simulate a bare
 * sandbox by deleting them" — this is the test that finally does, which the #158
 * follow-up asked for.
 *
 * It runs in a SUBPROCESS, not here. `bun test` shares one process across test
 * files and runs them concurrently, so deleting `TextEncoder` on this realm's
 * globalThis would break unrelated suites in ways that look like flakes. The
 * probe (`sandbox-compat-bare-probe.ts`) deletes the globals, then imports the
 * module, then prints JSON — so module evaluation is covered too.
 *
 * The expected values are computed HERE, in a realm that still has the platform
 * globals, so this is a real cross-check of the pure-JS paths against
 * `TextEncoder` / `TextDecoder` / `btoa` rather than the fallback agreeing with
 * itself.
 */

import { describe, expect, it } from 'bun:test';

const PROBE = new URL('./sandbox-compat-bare-probe.ts', import.meta.url).pathname;

interface ProbeResult {
  stillPresent: string[];
  encoded: number[][];
  roundTripped: string[];
  base64: string[];
  base64RoundTripped: string[];
  mixedByteLength: number;
  loneSurrogateBytes: number[];
  nowMsType: string;
  nowMsPositive: boolean;
  malformed: string;
  bomStripped: string;
}

// Same sample list as the probe, kept in step deliberately: the probe cannot
// import it (that would run in the bare realm) so both sides spell it out.
const SAMPLES = [
  '',
  'ascii $',
  String.fromCodePoint(0x00e9),
  String.fromCodePoint(0x4f60, 0x597d),
  String.fromCodePoint(0x1f389),
  `a${String.fromCodePoint(0x10ffff)}b`,
  `a${String.fromCharCode(0xd800)}b`,
];

async function runProbe(): Promise<ProbeResult> {
  const proc = Bun.spawn(['bun', PROBE], { stdout: 'pipe', stderr: 'pipe' });
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);
  if (exitCode !== 0) {
    throw new Error(`bare-sandbox probe exited ${exitCode}\nstderr:\n${stderr}`);
  }
  return JSON.parse(stdout.trim()) as ProbeResult;
}

describe('sandbox-compat in a bare sandbox (#158)', () => {
  it('every helper works with the platform globals absent from the start', async () => {
    const r = await runProbe();

    // The probe's own premise: the globals really are gone in its realm. Without
    // this the whole suite could pass while silently testing the platform paths.
    expect(r.stillPresent).toEqual([]);

    const encoder = new TextEncoder();
    const decoder = new TextDecoder('utf-8');

    // Encode: byte-identical to TextEncoder, computed in THIS realm.
    for (const [i, sample] of SAMPLES.entries()) {
      expect(r.encoded[i]).toEqual(Array.from(encoder.encode(sample)));
    }
    // The lone-surrogate rule the host's fatal wire decoders depend on.
    expect(r.loneSurrogateBytes).toEqual([0xef, 0xbf, 0xbd]);

    // Decode: round-trip. Only the lone-surrogate sample legitimately differs,
    // and it differs the same way TextDecoder(TextEncoder(x)) does.
    for (const [i, sample] of SAMPLES.entries()) {
      expect(r.roundTripped[i]).toBe(decoder.decode(encoder.encode(sample)));
    }

    // base64 both directions, checked against btoa and against a full round-trip.
    for (const [i, sample] of SAMPLES.entries()) {
      const bytes = encoder.encode(sample);
      expect(r.base64[i]).toBe(btoa(String.fromCharCode(...bytes)));
      expect(r.base64RoundTripped[i]).toBe(decoder.decode(bytes));
    }

    // 1 + 2 + 3 + 4 bytes for the four sequence lengths.
    expect(r.mixedByteLength).toBe(10);

    // nowMs falls back to Date.now() instead of throwing on absent performance.
    expect(r.nowMsType).toBe('number');
    expect(r.nowMsPositive).toBe(true);

    // Replacement rules survive the fallback: an overlong sequence is one U+FFFD
    // per maximal subpart, and the trailing ASCII is not eaten.
    expect(r.malformed).toBe(decoder.decode(new Uint8Array([0xe0, 0x80, 0x41])));
    expect(r.bomStripped).toBe('A');
  }, 30_000);
});
