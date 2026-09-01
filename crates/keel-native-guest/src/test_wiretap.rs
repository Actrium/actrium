//! Shared host-call wiretap harness for the SDK's wrapper tests.
//!
//! Runs on the HOST target (`cargo test -p keel-native-guest`): a test drives a REAL
//! host-call future against the recording `keel_host_call` shim, so it can read
//! the exact request bytes that would have crossed the seam, then feed a crafted
//! response back through the real `keel_resolve` path — a mock host, with none of
//! the wrapper's own logic stubbed out.
//!
//! Lives here rather than inside one test file because two suites need it:
//! `store_net_wrappers_tests` (KBS1 envelopes) and `module_verdict_tests`
//! (issue #165 — the body-level `ok` a wrapper must fold into its `Result`).

use crate::test_shims::CALLS;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use std::boxed::Box;
use std::string::String;
use std::vec::Vec;

pub(crate) fn poll_once<F: Future>(fut: Pin<&mut F>) -> Poll<F::Output> {
    let mut cx = Context::from_waker(Waker::noop());
    fut.poll(&mut cx)
}

/// A host call captured in-flight: the raw request bytes (for wiretapping) plus
/// the cb id, ready for `resolve` to feed a mock response back.
pub(crate) struct Wiretap<F: Future> {
    fut: Pin<Box<F>>,
    pub(crate) module: String,
    pub(crate) method: String,
    pub(crate) request: Vec<u8>,
    cb: u32,
}

/// Poll `f` once so it issues its `keel_host_call`, and capture that call.
/// Asserts the future parks (a wrapper that resolved without asking the host has
/// failed closed guest-side, which its own test should be checking instead).
pub(crate) fn issue<F: Future>(f: F) -> Wiretap<F> {
    CALLS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let mut fut = Box::pin(f);
    assert!(
        poll_once(fut.as_mut()).is_pending(),
        "host-call future must park on first poll"
    );
    let mut calls = CALLS.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(calls.len(), 1, "expected exactly one keel_host_call");
    let (module, method, request, cb) = calls.pop().expect("len checked");
    drop(calls);
    Wiretap {
        fut,
        module,
        method,
        request,
        cb,
    }
}

impl<F: Future> Wiretap<F> {
    /// Feed the mock host's response back through the real `keel_resolve` path
    /// and return the completed future's output. `ok` is the TRANSPORT status
    /// (1 = the module ran and answered); whatever verdict the module itself
    /// reports lives in `response`.
    pub(crate) fn resolve(mut self, ok: u32, response: &[u8]) -> F::Output {
        // Safety: `response` is a live slice we own for the duration of the call;
        // resolve copies it out immediately (to_vec).
        unsafe { crate::rt::resolve(self.cb, ok, response.as_ptr(), response.len()) };
        match poll_once(self.fut.as_mut()) {
            Poll::Ready(out) => out,
            Poll::Pending => panic!("future did not complete after resolve"),
        }
    }
}
