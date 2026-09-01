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
 * Assertions are on live native handles owned by an injected virtual scheduler,
 * not on callback spacing. Grid catch-up can legally deliver two ticks 1 ms
 * apart with a single chain (issue #121); a min-gap check cannot tell that
 * apart from the #107 double-arm.
 */

import { describe, expect, it } from 'bun:test';
import { TimerManager } from '../../engine/timer-manager';
import { VirtualTimerScheduler } from './virtual-timer-scheduler';

function makeOpts(clock: VirtualTimerScheduler) {
  return {
    debug: false,
    logger: { log: (..._a: unknown[]) => {}, error: (..._a: unknown[]) => {} },
    engineId: 'debug-gate',
    timing: clock.timing,
  };
}

function armInterval(
  clock: VirtualTimerScheduler,
  delay: number,
  onTick: () => void = () => {}
): { tm: TimerManager; id: number } {
  const tm = new TimerManager(makeOpts(clock));
  const id = tm.createSetIntervalPolyfill()(onTick, delay);
  clock.flushMicrotasks();
  return { tm, id };
}

/**
 * Arm, deliver the first tick into the gate, pause/resume `rounds` times,
 * flush each resume, then release the held tick.
 */
function runGatedPauseResume(rounds: number): {
  clock: VirtualTimerScheduler;
  tm: TimerManager;
  id: number;
  ticks: number;
} {
  const DELAY = 40;
  const clock = new VirtualTimerScheduler();
  let held: (() => void) | null = null;
  const tm = new TimerManager(makeOpts(clock));
  tm.setCallbackRunner((run) => {
    held = run;
  });

  let ticks = 0;
  const id = tm.createSetIntervalPolyfill()(() => {
    ticks += 1;
  }, DELAY);
  clock.flushMicrotasks();
  expect(clock.liveTimerCount).toBe(1);

  clock.deliverNext(DELAY);
  expect(held).not.toBeNull();
  expect(clock.liveTimerCount).toBe(0);
  expect(ticks).toBe(0);

  for (let i = 0; i < rounds; i++) {
    tm.pause();
    tm.resume();
    clock.flushMicrotasks();
    expect(clock.liveTimerCount).toBe(1);
  }

  tm.setCallbackRunner(null);
  held?.();
  expect(clock.liveTimerCount).toBe(1);
  expect(ticks).toBe(1);

  return { clock, tm, id, ticks };
}

function assertGatedSingleChain(rounds: number): void {
  const { clock, tm, id } = runGatedPauseResume(rounds);
  tm.pause();
  tm.resume();
  clock.flushMicrotasks();
  expect(clock.liveTimerCount).toBe(1);

  tm.createClearIntervalPolyfill()(id);
  expect(clock.liveTimerCount).toBe(0);
  tm.clearAllTimers();
}

function assertNoGateReArm(): void {
  const DELAY = 20;
  const WANT = 4;
  const clock = new VirtualTimerScheduler();
  let count = 0;
  const { tm, id } = armInterval(clock, DELAY, () => {
    count += 1;
  });

  for (let i = 0; i < WANT; i++) {
    clock.deliverNext();
    expect(clock.liveTimerCount).toBe(1);
  }
  expect(count).toBe(WANT);

  tm.createClearIntervalPolyfill()(id);
  expect(clock.liveTimerCount).toBe(0);
  tm.clearAllTimers();
}

describe('TimerManager: interval under debug gate + pause/resume', () => {
  it('runs one chain, not two, when pause/resume crosses a gated tick', () => {
    assertGatedSingleChain(1);
  });

  it('keeps a single chain across repeated pause/resume while gated', () => {
    assertGatedSingleChain(3);
  });

  it('still re-arms indefinitely with no gate installed', () => {
    assertNoGateReArm();
  });

  it('gated handle regressions stay deterministic across 500 virtual runs', () => {
    for (let i = 0; i < 500; i++) {
      assertGatedSingleChain(1);
      assertGatedSingleChain(3);
      assertNoGateReArm();
    }
  });
});
