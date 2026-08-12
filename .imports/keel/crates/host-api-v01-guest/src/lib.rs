//! Native Host API v0.1 end-to-end fixture.
//!
//! This deliberately uses only the public typed binding. The platform test
//! asserts that the resulting Wasm imports Keel's generic host-call/cancel ABI,
//! then observes the typed results through the real v0.1 store provider.
#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use core::future::{poll_fn, Future};
use core::task::Poll;
use keel_native_guest::host_api_v01::{
    clipboard, env, store, ByteSink, ColorScheme, StoreCondition,
};

keel_native_guest::guest_main!(guest_main);

const SOURCE_KEY: &str = "native-v01/source";
const REPORT_KEY: &str = "native-v01/report";
const SOURCE_BYTES: &[u8] = b"typed-rust-kbs1";

async fn guest_main() {
    let environment_ok = matches!(
        env::snapshot().await,
        Ok(snapshot)
            if snapshot.color_scheme == ColorScheme::Dark
                && snapshot.locale.language == "zh"
                && snapshot.locale.script.as_deref() == Some("Hans")
                && snapshot.time_zone_offset_minutes == 480
                && snapshot.accessibility.reduced_motion
                && !snapshot.accessibility.increased_contrast
    );

    let source_version = store::put(SOURCE_KEY, SOURCE_BYTES, StoreCondition::Absent)
        .await
        .ok()
        .map(|result| result.version);

    let mut output = [0u8; 64];
    let byte_sink_ok = match ByteSink::new(&mut output) {
        Ok(sink) => match store::get(SOURCE_KEY, sink).await {
            Ok(Some(result)) => {
                result.version == source_version.unwrap_or_default()
                    && result.written as usize == SOURCE_BYTES.len()
                    && result.required == result.written
                    && output[..result.written as usize] == *SOURCE_BYTES
            }
            _ => false,
        },
        Err(_) => false,
    };

    let structured_error_ok = match store::put(SOURCE_KEY, b"replace", StoreCondition::Absent).await
    {
        Err(error) => error.method == "store.put" && error.code == "conflict" && !error.retryable,
        Ok(_) => false,
    };

    // Poll exactly once so the host call crosses the Wasm ABI before cancel().
    // The test backend deliberately remains pending until its AbortSignal fires.
    let mut clipboard_output = [0u8; 64];
    let cancellation_ok = match ByteSink::new(&mut clipboard_output) {
        Ok(sink) => {
            let mut call = Box::pin(clipboard::read_text(sink));
            let mut completed_early = None;
            poll_fn(|context| {
                match call.as_mut().poll(context) {
                    Poll::Pending => {}
                    Poll::Ready(result) => completed_early = Some(result),
                }
                Poll::Ready(())
            })
            .await;

            let result = match completed_early {
                Some(result) => result,
                None => {
                    call.as_mut().get_mut().cancel();
                    call.await
                }
            };
            matches!(
                result,
                Err(error)
                    if error.method == "clipboard.readText"
                        && error.code == "cancelled"
                        && !error.retryable
            )
        }
        Err(_) => false,
    };

    let report = [
        environment_ok as u8,
        source_version.is_some() as u8,
        byte_sink_ok as u8,
        structured_error_ok as u8,
        cancellation_ok as u8,
    ];
    let _ = store::put(REPORT_KEY, &report, StoreCondition::Any).await;
}

/// A stable export lets the platform reject an accidentally substituted fixture.
#[no_mangle]
pub extern "C" fn host_api_v01_typed_fixture_marker() -> u32 {
    0x0001_4b53
}
