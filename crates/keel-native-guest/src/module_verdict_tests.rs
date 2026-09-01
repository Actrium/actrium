//! Tests for the TWO status levels a host reply carries (issue #165).
//!
//! `host_call` resolves with the TRANSPORT status: `ok == 1` means the host module
//! ran and answered. A module whose seam declares an `"ok":boolean` response also
//! reports its OWN verdict, in the body — so a refusal (`{"ok":false,"reason":…}`)
//! rides a perfectly successful call. Every wrapper over such a seam must fold
//! both levels into its `Result`, or `Ok` stops meaning "the operation happened".
//!
//! What that cost before the fix, measured downstream: a `present` past the host's
//! 8192-px cap returned `Ok`, the window went blank, and nothing was logged on
//! either side of the seam. So the load-bearing assertion in this file is the
//! NEGATIVE one — a refusal must NOT arrive as `Ok` — for each of the three
//! wrappers that mapped only the transport level (`canvas::present`,
//! `canvas::draw`, `gpu::submit`).
//!
//! Runs on the HOST target against the recording shim via `test_wiretap`: the
//! real wrapper, the real future, a mock host reply.

use crate::test_wiretap::issue;
use std::vec::Vec;

/// Every refusal shape a host can put in the body of a SUCCESSFUL call. The first
/// six are the reasons a real `host-canvas.ts` returns (over-dimension, geometry
/// mismatch, a canvas held in another mode, a missing context or memory reader);
/// the last two are hosts that fail closed without naming a reason at all.
const REFUSALS: &[&[u8]] = &[
    br#"{"ok":false,"reason":"invalid-input"}"#,
    br#"{"ok":false,"reason":"size-mismatch"}"#,
    br#"{"ok":false,"reason":"stride-mismatch"}"#,
    br#"{"ok":false,"reason":"mode-mismatch"}"#,
    br#"{"ok":false,"reason":"no-2d-context"}"#,
    br#"{"ok":false,"reason":"no-memory-reader"}"#,
    br#"{"ok":false}"#,
    br#"{"ok":false,"dropped":0}"#,
];

/// Replies that do NOT affirm `"ok":true`. A seam that declares the field and
/// omits it is contract-violating, and the SDK fails closed on it rather than
/// guessing — the same discipline `asset::blit` and `gpu::configure` already had.
const UNAFFIRMED: &[&[u8]] = &[
    br#"{}"#,
    br#"{"dropped":0}"#,
    br#"null"#,
    br#"{"ok":"true"}"#, // a STRING, not the boolean the seam declares
];

fn a_surface() -> crate::canvas::Surface {
    crate::canvas::Surface::new(2, 2)
}

fn a_draw_list() -> crate::canvas::DrawList {
    let mut list = crate::canvas::DrawList::new();
    list.fill_rect(0.0, 0.0, 1.0, 1.0);
    list
}

fn a_command_buffer() -> crate::gpu::CommandBuffer {
    let mut cb = crate::gpu::CommandBuffer::new();
    cb.begin_pass(0.0, 0.0, 0.0, 1.0)
        .draw(3)
        .end_pass()
        .finish();
    cb
}

/// Render a reply for an assertion message without pulling in `format!` machinery
/// at every call site.
fn shown(bytes: &[u8]) -> std::string::String {
    std::string::String::from_utf8_lossy(bytes).into_owned()
}

// ============================================================
// canvas::present — the reported case (a blank window, silently)
// ============================================================

#[test]
fn present_refusal_in_the_body_is_an_error_not_ok() {
    let _guard = crate::wire_lock();
    let surface = a_surface();
    for refusal in REFUSALS {
        let tap = issue(crate::canvas::present("c", &surface));
        assert_eq!(tap.module, "host:canvas");
        assert_eq!(tap.method, "present");
        // Transport says the call SUCCEEDED; only the body carries the refusal.
        let out = tap.resolve(1, refusal);
        let err = out.expect_err(&shown(refusal));
        // The host's own body — reason included — survives into Err untouched, so
        // a guest can still report WHY the frame did not land.
        assert_eq!(err, refusal.to_vec());
    }
}

#[test]
fn present_affirmed_frame_is_ok_with_the_body_intact() {
    let _guard = crate::wire_lock();
    let surface = a_surface();
    for reply in [
        &br#"{"ok":true}"#[..],
        &br#"{"ok":true,"dropped":0}"#[..],
        &br#"{"dropped":0,"ok":true}"#[..],
    ] {
        let tap = issue(crate::canvas::present("c", &surface));
        let out = tap.resolve(1, reply);
        assert_eq!(out.expect("affirmed present must be Ok"), reply.to_vec());
    }
}

#[test]
fn present_unaffirmed_reply_fails_closed() {
    let _guard = crate::wire_lock();
    let surface = a_surface();
    for reply in UNAFFIRMED {
        let tap = issue(crate::canvas::present("c", &surface));
        assert!(
            tap.resolve(1, reply).is_err(),
            "reply {} does not affirm ok:true and must fail closed",
            shown(reply)
        );
    }
}

#[test]
fn present_transport_failure_is_still_an_error() {
    let _guard = crate::wire_lock();
    let surface = a_surface();
    // ok == 0: the module never ran (unregistered capability, JS guest, teardown).
    // Distinguishable from a refusal by the ABSENCE of an "ok" field in the body.
    let tap = issue(crate::canvas::present("c", &surface));
    let err = tap
        .resolve(0, b"denied")
        .expect_err("transport failure must be Err");
    assert_eq!(err, b"denied".to_vec());
}

/// The reason a refused frame cannot be shrugged off as a documentation problem:
/// the shape a guest naturally writes is `let _ = present(..)`, and with the old
/// mapping that discarded a refusal. `Result` is `#[must_use]`, so with the
/// refusal folded in, ignoring it is at least a deliberate act — and `?` now
/// propagates it.
#[test]
fn a_guest_using_the_question_mark_operator_sees_the_refusal() {
    let _guard = crate::wire_lock();
    let surface = a_surface();
    fn frame(reply: Result<Vec<u8>, Vec<u8>>) -> Result<(), Vec<u8>> {
        reply?; // the whole point: a refused frame must stop the happy path here
        Ok(())
    }
    let tap = issue(crate::canvas::present("c", &surface));
    let refused = tap.resolve(1, br#"{"ok":false,"reason":"invalid-input"}"#);
    assert!(frame(refused).is_err());
}

// ============================================================
// canvas::draw — same seam, same two levels
// ============================================================

/// `draw` has TWO encoding paths (binary `KCNV` frame vs legacy JSON op-list) and
/// each mapped its reply separately, so each is checked. The handshake cache is
/// primed explicitly — see `set_binary_supported_for_test` for why leaving it to
/// chance makes a `draw` wiretap order-dependent.
#[test]
fn draw_refusal_in_the_body_is_an_error_not_ok_on_both_encodings() {
    let _guard = crate::wire_lock();
    let list = a_draw_list();
    for binary in [false, true] {
        crate::canvas::set_binary_supported_for_test(Some(binary));
        for refusal in REFUSALS {
            let tap = issue(crate::canvas::draw("c", &list));
            assert_eq!(
                tap.method, "draw",
                "binary={binary}: expected the draw call"
            );
            assert_eq!(
                tap.resolve(1, refusal).expect_err(&shown(refusal)),
                refusal.to_vec(),
                "binary={binary}"
            );
        }
    }
    crate::canvas::set_binary_supported_for_test(None);
}

#[test]
fn draw_accepted_list_with_dropped_ops_stays_ok() {
    let _guard = crate::wire_lock();
    // `dropped` on an AFFIRMED reply is partial success — the host replayed the
    // list and skipped n unknown ops. That must stay `Ok`; only `ok:false` is a
    // refusal of the whole call.
    crate::canvas::set_binary_supported_for_test(Some(false));
    let list = a_draw_list();
    let tap = issue(crate::canvas::draw("c", &list));
    let reply = br#"{"ok":true,"dropped":3}"#;
    assert_eq!(
        tap.resolve(1, reply).expect("partial success is Ok"),
        reply.to_vec()
    );
    crate::canvas::set_binary_supported_for_test(None);
}

// ============================================================
// gpu::submit — its doc already promised this; the code did not
// ============================================================

#[test]
fn submit_refusal_in_the_body_is_an_error_not_ok() {
    let _guard = crate::wire_lock();
    for refusal in [
        &br#"{"ok":false,"reason":"unknown-canvas"}"#[..],
        &br#"{"ok":false,"reason":"over-budget"}"#[..],
        &br#"{"ok":false,"reason":"device-lost"}"#[..],
    ] {
        let cmds = a_command_buffer();
        let tap = issue(crate::gpu::submit("c", &cmds));
        assert_eq!(tap.module, "host:gpu");
        assert_eq!(tap.method, "submit");
        assert_eq!(
            tap.resolve(1, refusal).expect_err(&shown(refusal)),
            refusal.to_vec()
        );
    }
}

#[test]
fn submit_affirmed_is_ok() {
    let _guard = crate::wire_lock();
    let cmds = a_command_buffer();
    let tap = issue(crate::gpu::submit("c", &cmds));
    let reply = br#"{"ok":true,"dropped":0}"#;
    assert_eq!(
        tap.resolve(1, reply).expect("affirmed submit is Ok"),
        reply.to_vec()
    );
}

// ============================================================
// the flag reader itself
// ============================================================

#[test]
fn json_flag_true_reads_only_an_affirmative_boolean() {
    assert!(crate::json_flag_true(br#"{"ok":true}"#, "ok"));
    assert!(crate::json_flag_true(br#"{"dropped":0,"ok":true}"#, "ok"));
    assert!(!crate::json_flag_true(br#"{"ok":false}"#, "ok"));
    assert!(!crate::json_flag_true(br#"{}"#, "ok"));
    assert!(!crate::json_flag_true(br#"{"okay":true}"#, "ok"));
    assert!(!crate::json_flag_true(br#"{"ok":"true"}"#, "ok"));
    // Invalid UTF-8 cannot be read, so it cannot affirm.
    assert!(!crate::json_flag_true(&[0xff, 0xfe], "ok"));
    // The one way a substring scan could false-affirm: the literal `"ok":true`
    // inside a reason string, alongside the real verdict. An explicit false
    // vetoes, so this lands on the fail-closed side.
    assert!(!crate::json_flag_true(
        br#"{"ok":false,"reason":"expected \"ok\":true"}"#,
        "ok"
    ));
}
