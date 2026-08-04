/**
 * TimerManager under a debug callback gate.
 *
 * The gate is what the worker debug harness installs (`setCallbackRunner`) so a
 * timer callback cannot re-enter the guest while a turn is suspended at a
 * breakpoint. It opens a window the release path never has: the native timer
 * has already fired, but its callback has not run yet. A pause/resume landing
 * in that window used to arm a second native timer whose id was then overwritten
 * by the held tick's own re-arm. Neither could cancel the other, so the interval
 * ran as two chains at once -- and every further pause/resume added another
 * (issue #107).
 *
 * The assertions are on tick SPACING, and each phase waits for a tick COUNT
 * rather than for a stretch of wall clock. Both choices are deliberate: an
 * extra chain shows up as two ticks landing milliseconds apart, a signature a
 * loaded machine cannot fake, whereas "how many ticks fit in 80 ms" says more
 * about the CI runner than about the timer (see the wall-clock cleanup in #97).
 * Under-delivery is caught by the wait never completing, not by an upper bound.
 */

import { describe, expect, it } from 'bun:test';
import { TimerManager } from '../../engine/timer-manager';

function makeOpts() {
  return {
    debug: false,
    logger: { log: (..._a: unknown[]) => {}, error: (..._a: unknown[]) => {} },
    engineId: 'debug-gate',
  };
}

const tick = () => new Promise((r) => setTimeout(r, 0));
const after = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Smallest gap between consecutive ticks, or Infinity for fewer than two. */
function minGap(stamps: number[]): number {
  let min = Number.POSITIVE_INFINITY;
  for (let i = 1; i < stamps.length; i++) {
    min = Math.min(min, stamps[i] - stamps[i - 1]);
  }
  return min;
}

/**
 * Drive one interval through `rounds` pause/resume cycles while a gate holds
 * its first tick, then release the gate and wait until `wantTicks` have landed.
 * Returns the tick timestamps.
 */
async function runGatedIntervalScenario(delay: number, rounds: number, wantTicks: number) {
  const tm = new TimerManager(makeOpts());

  let held: (() => void) | null = null;
  tm.setCallbackRunner((run) => {
    held = run;
  });

  const stamps: number[] = [];
  let reachedTarget: (() => void) | null = null;
  const target = new Promise<void>((resolve) => {
    reachedTarget = resolve;
  });

  tm.createSetIntervalPolyfill()(() => {
    stamps.push(Date.now());
    if (stamps.length >= wantTicks) reachedTarget?.();
  }, delay);

  await tick(); // arm the first native timer
  await after(delay + 15); // it fires; the gate grabs the callback
  const gatedTick = held as unknown as (() => void) | null;
  held = null;

  for (let i = 0; i < rounds; i++) {
    tm.pause();
    tm.resume();
    await tick(); // let the resume's replacement timer actually arm
  }

  tm.setCallbackRunner(null); // ungate: anything from here runs directly
  gatedTick?.(); // the held tick finally runs, and re-arms

  // No wall-clock budget: a chain that stopped re-arming fails by never getting
  // here, which the test timeout reports.
  await target;
  tm.clearAllTimers(); // stop the chain so it cannot bleed into the next test
  return { tm, stamps, gatedTick };
}

describe('TimerManager: interval under debug gate + pause/resume', () => {
  it('runs one chain, not two, when pause/resume crosses a gated tick', async () => {
    const DELAY = 40;
    const { stamps, gatedTick } = await runGatedIntervalScenario(DELAY, 1, 4);

    expect(gatedTick).not.toBeNull();
    // An orphaned second chain showed up here as ticks a few ms apart.
    expect(minGap(stamps)).toBeGreaterThan(DELAY / 2);
  });

  it('keeps a single chain across repeated pause/resume while gated', async () => {
    const DELAY = 30;
    // Each round used to orphan one more armed timer, so the tick rate grew
    // with the number of pause/resume cycles.
    const { stamps } = await runGatedIntervalScenario(DELAY, 3, 4);

    expect(minGap(stamps)).toBeGreaterThan(DELAY / 2);
  });

  it('still re-arms indefinitely with no gate installed', async () => {
    const tm = new TimerManager(makeOpts());

    const DELAY = 20;
    const WANT = 4;
    const stamps: number[] = [];
    let reachedTarget: (() => void) | null = null;
    const target = new Promise<void>((resolve) => {
      reachedTarget = resolve;
    });

    const id = tm.createSetIntervalPolyfill()(() => {
      stamps.push(Date.now());
      if (stamps.length >= WANT) reachedTarget?.();
    }, DELAY);

    // The re-arm now clears the just-fired handle before arming the next one.
    // Clearing the WRONG handle there would stop the chain dead, so the check
    // that matters is that the chain keeps going -- reaching WANT at all. How
    // long it takes is the runner's business, not the timer's.
    await target;
    tm.createClearIntervalPolyfill()(id);

    expect(stamps.length).toBeGreaterThanOrEqual(WANT);
    expect(minGap(stamps)).toBeGreaterThan(DELAY / 2);
  });
});
