//! Test-fixture native (non-JS) keel guest that exercises the WIP binary
//! OP-BATCH wire end-to-end via the `keel_wire_caps` runtime gate.
//!
//! Built with `keel-native-guest`'s `binary-protocol` feature ON, so the SDK
//! exports `keel_wire_caps`:
//!   - a host that pushes bit0 (binaryOpBatch on) BEFORE `keel_init` receives
//!     the render batch as a binary `KEEL` frame;
//!   - any other host (old host, option off) never calls the export, the gate
//!     stays false, and the guest transparently emits the legacy JSON batch —
//!     the graceful-degrade path.
//!
//! The tree is the SAME deterministic one `ui-guest` renders, so a host
//! round-trip test can assert the decoded `OperationBatch` equals the JSON
//! guest's byte-for-byte in op terms. It deliberately repeats node types
//! (`View`, `__TEXT__`) so the wire's intern table is exercised (one entry,
//! multiple refs).
#![no_std]

extern crate alloc;

use keel_native_guest::ui::{text, view};

keel_native_guest::guest_main!(guest_main);

async fn guest_main() {
    keel_native_guest::render(view(alloc::vec![
        text("hello from rust"),
        view(alloc::vec![text("nested")]),
    ]));
}
