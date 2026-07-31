//! Demo native (non-JS) rill guest that renders UI. It builds a small element
//! tree with the `rill-native-guest` SDK and hands the host a render batch, which the
//! Phase A `NativeGuestHost` forwards to the real `receiver` to materialize — the
//! first time a native guest produces UI. Compiles to a `.wasm` loaded unchanged.
#![no_std]

extern crate alloc;

use rill_native_guest::ui::{text, view};

rill_native_guest::guest_main!(guest_main);

async fn guest_main() {
    rill_native_guest::render(view(alloc::vec![
        text("hello from rust"),
        view(alloc::vec![text("nested")]),
    ]));
}
