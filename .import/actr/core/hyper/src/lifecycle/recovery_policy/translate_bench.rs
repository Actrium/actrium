//! M3 — pure `translate()` throughput micro-benchmark.
//!
//! `translate`, `View`, and `Input` are `pub(crate)`: they are the RFC-0400
//! policy core's internal vocabulary, deliberately not part of the crate's
//! public API. A `benches/*.rs` binary is a separate compilation unit that
//! only links against `actr_hyper`'s public surface, so it cannot name any of
//! these types — `core/hyper/benches/recovery_latency.rs` (M1/M2) documents
//! this and points here.
//!
//! This lives as an in-tree, `#[ignore]`d unit test instead: a child module
//! of `translate`, so it has the same `super::*` access as
//! `translate_tests.rs`. It is deliberately *not* a normal `#[test]` that
//! runs in `cargo test` — it is a throughput measurement, not a correctness
//! check — hence `#[ignore]`.
//!
//! ## Run it
//!
//! ```text
//! cargo test --release -p actr-hyper --lib translate_bench:: -- --ignored --nocapture
//! ```
//!
//! ## Method
//!
//! One fixed `View` (a representative mid-flight snapshot: active/foreground/
//! online, a Reconnect effect in flight at revision 5) and one representative
//! mix of seven `Input`s spanning the categories the task calls out —
//! `NetworkSnapshot` online/offline, `AppEnteredForeground`/`Background`,
//! `EffectCompleted` success/failure, and `RecoveryRequested` — are built
//! once. The `View` is never mutated by applying the returned `Decision`
//! (translation is queried, not driven), so every call measures the same
//! `translate()` cost regardless of accumulated policy state; this isolates
//! the reducer's own per-call cost from the surrounding supervisor's
//! bookkeeping (which M1/M2 already cover end-to-end).
//!
//! Each round calls `translate()` `ITERS_PER_ROUND` times cycling through the
//! seven inputs, accumulating a cheap checksum of each `Decision`'s shape
//! into `std::hint::black_box` so the optimizer cannot prove the result is
//! unused and elide the calls. `ROUNDS` rounds are timed independently; the
//! median round's rate is the reported throughput.

use std::hint::black_box;
use std::time::Instant;

use super::super::classification::FixedEntropy;
use super::*;

const ITERS_PER_ROUND: usize = 300_000;
const ROUNDS: usize = 7;

#[derive(Debug)]
struct BenchmarkResult {
    checksum: u64,
    round_rates: Vec<f64>,
}

fn representative_view() -> View {
    let mut view = View::initial();
    view.recovery_mode = RecoveryModeState::Active;
    view.app_phase = AppPhaseState::Foreground;
    view.network_path = NetworkPathState::Online;
    view.recovery_intent = RecoveryIntentState::ReconnectPending;
    view.execution = ExecutionState::Reconnecting;
    view.policy_revision = 5;
    view.recovery_record = Some(PendingRecord::recovery(5, RecoveryStrength::Reconnect));
    view.effect = Some(EffectContext {
        action_id: 1,
        kind: EffectKind::Reconnect,
        captured_revision: 5,
        cancel_reason: None,
    });
    view.last_snapshot = Some(AcceptedSnapshot {
        source_epoch: 1,
        sequence: 1,
        semantic_path: SemanticPath::Unknown,
        route_fingerprint: 0,
    });
    view.live_signaling_generation = Some(3);
    view
}

/// Seven inputs: `NetworkSnapshot` online/offline, `AppEntered{Fore,Back}ground`,
/// `EffectCompleted` success/failure, `RecoveryRequested` — the categories the
/// spec calls out.
fn representative_inputs() -> Vec<Input> {
    vec![
        Input::NetworkSnapshot {
            source_epoch: 1,
            sequence: 2,
            observed_at: Duration::ZERO,
            semantic_path: SemanticPath::Online,
            route_fingerprint: 1,
        },
        Input::NetworkSnapshot {
            source_epoch: 1,
            sequence: 3,
            observed_at: Duration::ZERO,
            semantic_path: SemanticPath::Offline,
            route_fingerprint: 1,
        },
        Input::AppEnteredForeground {
            observed_background_duration: None,
        },
        Input::AppEnteredBackground,
        Input::RecoveryRequested {
            minimum: RecoveryStrength::Reconnect,
            reason: RecoveryRequestReason::ManualReconnect,
        },
        Input::EffectCompleted {
            action_id: 1,
            kind: EffectKind::Reconnect,
            policy_revision: 5,
            outcome: EffectOutcome::Succeeded,
        },
        Input::EffectCompleted {
            action_id: 1,
            kind: EffectKind::Reconnect,
            policy_revision: 5,
            outcome: EffectOutcome::Failed {
                diagnosis: EffectDiagnosis::PathUnreachable {
                    stage: "probe".to_string(),
                },
            },
        },
    ]
}

fn run_representative_mix(iters_per_round: usize, rounds: usize) -> BenchmarkResult {
    assert!(
        iters_per_round > 0,
        "benchmark needs at least one iteration"
    );
    assert!(rounds > 0, "benchmark needs at least one round");

    let view = representative_view();
    let inputs = representative_inputs();
    let config = PolicyConfig::defaults();
    let now = Duration::from_secs(1);

    let mut checksum: u64 = 0;
    let mut round_rates = Vec::with_capacity(rounds);
    for round in 0..rounds {
        let mut entropy = FixedEntropy::constant(0.37);
        let mut sink: u64 = 0;
        let start = Instant::now();
        for i in 0..iters_per_round {
            let input = &inputs[i % inputs.len()];
            let d = translate(&view, input, now, &config, &mut entropy);
            sink = sink
                .wrapping_add(d.machine_inputs.len() as u64)
                .wrapping_add(d.timers.len() as u64)
                .wrapping_add(d.gate_triggers.len() as u64)
                .wrapping_add(d.cancels.len() as u64);
        }
        let elapsed = start.elapsed();
        checksum = checksum.wrapping_add(black_box(sink));
        let rate = iters_per_round as f64 / elapsed.as_secs_f64();
        let ns_per_decision = elapsed.as_nanos() as f64 / iters_per_round as f64;
        println!(
            "round {}: {:?} total, {:.0} decisions/sec, {:.1} ns/decision",
            round + 1,
            elapsed,
            rate,
            ns_per_decision
        );
        round_rates.push(rate);
    }

    BenchmarkResult {
        checksum,
        round_rates,
    }
}

fn report_median(result: &mut BenchmarkResult) {
    result
        .round_rates
        .sort_by(|a, b| a.partial_cmp(b).expect("no NaN rates"));
    let median_rate = result.round_rates[result.round_rates.len() / 2];
    println!(
        "median: {:.0} decisions/sec ({:.1} ns/decision)",
        median_rate,
        1e9 / median_rate
    );
}

#[test]
fn representative_mix_smoke_covers_every_input_class() {
    let input_count = representative_inputs().len();
    let mut result = run_representative_mix(input_count * 2, 3);

    assert!(result.checksum > 0, "decisions should contribute output");
    assert_eq!(result.round_rates.len(), 3);
    assert!(
        result
            .round_rates
            .iter()
            .all(|rate| rate.is_finite() && *rate > 0.0),
        "every benchmark round should produce a finite positive rate"
    );
    report_median(&mut result);
}

#[test]
#[ignore = "throughput microbenchmark, not a correctness check; \
            run with `--release ... -- --ignored --nocapture`"]
fn translate_throughput_representative_mix() {
    let mut result = run_representative_mix(ITERS_PER_ROUND, ROUNDS);
    report_median(&mut result);
}
