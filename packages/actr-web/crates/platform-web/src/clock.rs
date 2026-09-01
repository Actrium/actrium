//! Web monotonic clock, RFC-0419 §2.
//!
//! Browser backend of the [`MonotonicClock`] trait. The reading comes from
//! `performance.now()` (the High Resolution Time API): a per-origin, monotonic
//! counter that — unlike `Date.now()` / `SystemTime` — is not slewed or stepped by
//! NTP or manual clock changes. Time spent in a background tab is *throttled*, not
//! advanced in real time, so a deadline may fire late there; the §2 contract allows
//! arbitrary late firing (expiry is guaranteed; the firing instant is a scheduling
//! property).
//!
//! # Instant representation
//!
//! [`WebInstant`] stores whole milliseconds since the Performance timeline origin.
//! The `MonotonicClock` contract explicitly guarantees **no sub-millisecond
//! precision** — browsers deliberately coarsen `performance.now()` as a Spectre
//! mitigation — so a coarser unit loses nothing callers may rely on, and keeps
//! generous `u64` headroom (Chrome caps the reading near 8.64e15 ms; as µs that
//! would consume most of the `u64` range and erode the overflow sentinel's distance).
//!
//! # Failure mode
//!
//! `now()` panics if the Performance API is unavailable. This mirrors the native
//! Windows backend, which `assert!`s on a failed `QueryUnbiasedInterruptTime`: the
//! trait offers no error path, and silently fabricating time would let a missing
//! clock fire deadlines early or stall them indefinitely. The Performance API is
//! available in every context this crate targets (Window, Worker, ServiceWorker).
//!
//! # Not a clock for suspend-aware durations
//!
//! Like every [`MonotonicClock`], this is process-local relative time only —
//! "how much later is this deadline?", never "what time is it?". Uses that must
//! measure real elapsed time across device suspend (long-background liveness) are a
//! different clock class and must not be built on this trait.

use std::time::Duration;

use actr_platform_traits::MonotonicClock;
use wasm_bindgen::JsCast;
use web_sys::WorkerGlobalScope;

/// Reading of [`WebMonotonicClock`]: whole milliseconds since the Performance
/// timeline origin.
///
/// Like every [`MonotonicClock::Instant`], values are process- and clock-local:
/// they carry no calendar meaning, must not be serialized or sent on the wire, do
/// not survive a page/context reload, and must not be compared across execution
/// contexts (e.g. a Worker and the main thread, which have independent origins).
///
/// `u64::MAX` ms (~584,000 years) is the overflow sentinel of
/// [`WebMonotonicClock::add`]: not earlier than any reachable reading, and
/// absorbing — `saturating_add` keeps every further `add` at `u64::MAX`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WebInstant(u64);

/// Web monotonic clock backed by `performance.now()`.
///
/// Upholds the [`MonotonicClock`] contract:
///
/// - `elapsed` saturates to [`Duration::ZERO`] when `later < earlier`;
/// - `add` saturates on overflow to a never-firing, absorbing sentinel.
///
/// See the module docs for the instant representation and failure-mode rationale.
#[derive(Clone, Copy, Debug, Default)]
pub struct WebMonotonicClock;

/// Obtain the global `Performance` object.
///
/// Tries the Window context first, then falls back to the Worker / Service Worker
/// global scope. Panics if neither exposes the Performance API — see the module
/// failure-mode rationale.
fn global_performance() -> web_sys::Performance {
    web_sys::window()
        .and_then(|w| w.performance())
        .or_else(|| {
            js_sys::global()
                .dyn_into::<WorkerGlobalScope>()
                .ok()
                .and_then(|w| w.performance())
        })
        .expect(
            "Performance API unavailable; cannot enforce a monotonic deadline. \
             actr-platform-web requires the High Resolution Time API \
             (Window/Worker/ServiceWorkerGlobalScope.performance).",
        )
}

/// Convert a [`Duration`] to whole milliseconds, rounding up: a deadline computed
/// from a sub-millisecond duration may only fire later than requested, never
/// earlier ("fires no earlier than" semantics). Out-of-range durations clamp to
/// `u64::MAX`, the overflow sentinel.
fn duration_to_ms_ceil(duration: Duration) -> u64 {
    let nanos = duration.as_nanos();
    let ms = nanos / 1_000_000 + u128::from(!nanos.is_multiple_of(1_000_000));
    u64::try_from(ms).unwrap_or(u64::MAX)
}

impl MonotonicClock for WebMonotonicClock {
    type Instant = WebInstant;

    fn now(&self) -> WebInstant {
        let ms = global_performance().now();
        // `performance.now()` is non-negative per spec; clamp defensively so a
        // misbehaving host can never yield a negative reading (which `as u64`
        // would saturate to a huge value).
        WebInstant(ms.max(0.0) as u64)
    }

    fn add(&self, instant: WebInstant, duration: Duration) -> WebInstant {
        // Saturation lands on u64::MAX, the absorbing never-firing sentinel.
        WebInstant(instant.0.saturating_add(duration_to_ms_ceil(duration)))
    }

    fn elapsed(&self, earlier: WebInstant, later: WebInstant) -> Duration {
        Duration::from_millis(later.0.saturating_sub(earlier.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    use gloo_timers::future::TimeoutFuture;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    /// §6 Web leg: the same executable contract suite the virtual `TestClock` and
    /// the native `NativeMonotonicClock` pass, now run against the Web clock, so
    /// deadline code written against the trait behaves identically on every runtime.
    #[wasm_bindgen_test]
    fn web_clock_upholds_the_monotonic_clock_contract() {
        actr_platform_traits::assert_monotonic_clock_contract(&WebMonotonicClock);
    }

    /// "Fires no earlier than" against the real browser clock: after a `setTimeout`
    /// wait — which the HTML spec guarantees fires no earlier than its delay — the
    /// monotonic elapsed time is at least the requested duration. The Web analog of
    /// the native real-wait deadline test in `crates/actr-platform-native/src/clock.rs`.
    #[wasm_bindgen_test(async)]
    async fn elapsed_across_a_real_wait_is_at_least_the_requested_duration() {
        let clock = WebMonotonicClock;
        let requested = Duration::from_millis(10);

        let start = clock.now();
        TimeoutFuture::new(10).await;
        let elapsed = clock.elapsed(start, clock.now());

        assert!(
            elapsed >= requested,
            "monotonic elapsed across a {requested:?} wait was only {elapsed:?}; \
             a deadline computed from this clock would have fired early"
        );
    }

    /// Whole-millisecond granularity rounds sub-ms durations up, never down: a 1 ns
    /// deadline lands one whole ms later, so it can only fire after the requested
    /// instant. Parity with the Windows `sub_tick_durations_round_up_never_down`
    /// test, pinning the round-up policy that underpins the "no earlier than"
    /// guarantee.
    #[wasm_bindgen_test]
    fn sub_millisecond_durations_round_up_never_down() {
        let clock = WebMonotonicClock;
        let t = clock.now();

        let deadline = clock.add(t, Duration::from_nanos(1));
        assert!(deadline > t, "a 1 ns deadline must land strictly later");
        assert_eq!(
            clock.elapsed(t, deadline),
            Duration::from_millis(1),
            "1 ns rounds up to one whole millisecond"
        );

        // Exact whole-ms multiples stay exact.
        let exact = clock.add(t, Duration::from_millis(1));
        assert_eq!(clock.elapsed(t, exact), Duration::from_millis(1));
    }
}
