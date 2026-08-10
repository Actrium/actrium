//! Tests for the `Reply<T>` handle (RFC-0323) — idempotent completer, Drop
//! fallback, and the TELL empty-variant.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use actr_protocol::ActrError;
use prost::Message as ProstMessage;

use super::{Deliver, Reply, ReplyOutcome};

/// A `Deliver` impl that records every delivered outcome in a shared Vec.
#[derive(Default)]
struct CapturingDeliver {
    delivered: Mutex<Vec<ReplyOutcome>>,
}

impl CapturingDeliver {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn outcomes(&self) -> Vec<ReplyOutcome> {
        self.delivered.lock().expect("lock").clone()
    }
}

impl Deliver for CapturingDeliver {
    fn deliver(&self, outcome: ReplyOutcome) {
        self.delivered.lock().expect("lock").push(outcome);
    }
}

/// A minimal prost message to exercise the `Reply<T>` encode path.
#[derive(Clone, PartialEq, ProstMessage)]
struct Resp {
    #[prost(string, tag = "1")]
    msg: String,
}

fn make_reply(deliver: Arc<dyn Deliver>, emit_response: bool) -> Reply<Resp> {
    Reply::new(
        "req-1".to_string(),
        Some(Instant::now()),
        deliver,
        emit_response,
    )
}

#[test]
fn send_owns_happily_and_delivers_once() {
    let cap = CapturingDeliver::new();
    let reply = make_reply(cap.clone(), true);
    reply.send(Ok(Resp { msg: "hi".into() }));
    // After `send` the handle is consumed; the completer recorded exactly one
    // delivery.
    let outs = cap.outcomes();
    assert_eq!(outs.len(), 1);
    let o = &outs[0];
    assert_eq!(o.request_id, "req-1");
    assert!(o.emit_response);
    let bytes = o.result.as_ref().expect("ok").clone();
    let decoded = Resp::decode(&*bytes).expect("decode");
    assert_eq!(decoded.msg, "hi");
}

#[test]
fn drop_without_send_emits_internal_error_fallback() {
    let cap = CapturingDeliver::new();
    {
        let _reply = make_reply(cap.clone(), true);
        // dropped without send
    }
    let outs = cap.outcomes();
    assert_eq!(outs.len(), 1, "Drop must complete the handle exactly once");
    let o = &outs[0];
    assert!(o.emit_response);
    assert!(matches!(
        o.result,
        Err(ActrError::Internal(ref m)) if m.contains("reply dropped without response")
    ));
}

#[test]
fn tell_empty_variant_completes_with_empty_bytes_and_no_response() {
    let cap = CapturingDeliver::new();
    {
        let _reply = make_reply(cap.clone(), false);
    }
    let outs = cap.outcomes();
    assert_eq!(outs.len(), 1);
    let o = &outs[0];
    assert!(!o.emit_response, "TELL variant must not emit a RESPONSE");
    assert!(o.result.is_ok());
    assert!(o.result.as_ref().unwrap().is_empty());
}

#[test]
fn send_then_drop_is_exactly_one_delivery() {
    let cap = CapturingDeliver::new();
    {
        let reply = make_reply(cap.clone(), true);
        reply.send(Ok(Resp {
            msg: "first".into(),
        }));
        // After send, the completer is completed. Dropping the *consumed*
        // handle is not possible (send took self), but the spawned-task
        // twin race is modelled by completing the shared completer then
        // observing a second handle drop is not the path — instead the
        // framework error path / a second completer clone would race.
    }
    assert_eq!(cap.outcomes().len(), 1);
}

/// The §3 double-reply race (spawned task holds the `Reply` while the handler
/// returns `Err`, so the framework error path races `send`) is resolved at the
/// *completer* level: first writer wins, late writers are no-ops. The per-handle
/// invariant is "exactly one delivery" — `send` and `Drop` are mutually
/// exclusive because `send` consumes `self`. The cross-handle twin race is the
/// pump's job (it keeps its own `Arc<Completer>`), not the Reply type's.
#[test]
fn send_completes_the_completer_exactly_once() {
    let cap = CapturingDeliver::new();
    let reply = make_reply(cap.clone(), true);
    reply.send(Ok(Resp { msg: "wins".into() }));
    assert_eq!(cap.outcomes().len(), 1);
}

#[test]
fn deadline_is_stored_and_returned() {
    let cap = CapturingDeliver::new();
    let dl = Instant::now();
    let reply = Reply::new("req-dl".to_string(), Some(dl), cap, true);
    assert_eq!(reply.deadline(), Some(dl));
    assert_eq!(reply.request_id(), "req-dl");
    reply.send(Ok(Resp { msg: "done".into() }));
}

#[test]
fn tell_variant_send_completes_without_response() {
    let cap = CapturingDeliver::new();
    let reply = make_reply(cap.clone(), false);
    reply.send(Ok(Resp {
        msg: "ignored".into(),
    }));
    let outs = cap.outcomes();
    assert_eq!(outs.len(), 1);
    assert!(!outs[0].emit_response);
    // payload still encoded (calmly), but the runtime would not emit it
    assert!(outs[0].result.is_ok());
}

#[test]
fn send_error_delivers_err_result() {
    let cap = CapturingDeliver::new();
    let reply = make_reply(cap.clone(), true);
    reply.send(Err(ActrError::Internal("boom".to_string())));
    let outs = cap.outcomes();
    assert_eq!(outs.len(), 1);
    assert!(outs[0].emit_response);
    assert!(matches!(
        outs[0].result,
        Err(ActrError::Internal(ref m)) if m == "boom"
    ));
}

#[test]
fn reply_is_send_and_static() {
    fn assert_send_static<T: Send + 'static>() {}
    assert_send_static::<Reply<Resp>>();
}
