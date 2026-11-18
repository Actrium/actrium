use super::*;
use std::sync::mpsc as blocking_channel;
use tokio::time::{Instant, advance};

const TEST_TIMEOUT: Duration = Duration::from_secs(5);

#[test]
fn queued_wasi_preopen_timeout_releases_slot_before_worker_is_available() {
    static SLOTS: Semaphore = Semaphore::const_new(1);
    let workers = Builder::new_current_thread()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    let runtime = Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .unwrap();
    runtime.block_on(async {
        let (ready, started) = oneshot::channel();
        let (unblock, blocked) = blocking_channel::channel::<()>();
        let occupying = workers.spawn_blocking(move || {
            ready.send(()).unwrap();
            let _ = blocked.recv();
        });
        started.await.unwrap();

        let (called, call) = oneshot::channel();
        let open = run_wasi_preopen(
            workers.handle(),
            &SLOTS,
            Instant::now() + TEST_TIMEOUT,
            TEST_TIMEOUT,
            || {
                let _ = called.send(());
                Ok(WasiFilesystemCtx::default())
            },
        );
        tokio::pin!(open);
        assert!(futures_util::poll!(&mut open).is_pending());
        assert_eq!(SLOTS.available_permits(), 0);
        advance(TEST_TIMEOUT + Duration::from_millis(1)).await;
        assert!(matches!(open.await, Err(WasmError::InvocationTimeout(_))));
        assert_eq!(SLOTS.available_permits(), 1);

        drop(unblock);
        occupying.await.unwrap();
        assert!(call.await.is_err());
    });
}

#[test]
fn started_wasi_preopen_timeout_does_not_block_mount_free_attach() {
    static SLOTS: Semaphore = Semaphore::const_new(1);
    let caller = Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .max_blocking_threads(1)
        .build()
        .unwrap();
    caller.block_on(async {
        let workers = wasi_preopen_workers().unwrap();
        let (ready, started) = blocking_channel::channel();
        let (unblock, blocked) = blocking_channel::channel::<()>();
        let open = run_wasi_preopen(
            workers,
            &SLOTS,
            Instant::now() + TEST_TIMEOUT,
            TEST_TIMEOUT,
            move || {
                ready.send(()).unwrap();
                let _ = blocked.recv();
                Ok(WasiFilesystemCtx::default())
            },
        );
        tokio::pin!(open);
        assert!(futures_util::poll!(&mut open).is_pending());
        // Keep virtual time still until the blocking worker owns the permit.
        started.recv_timeout(TEST_TIMEOUT).unwrap();
        advance(TEST_TIMEOUT + Duration::from_millis(1)).await;
        assert!(matches!(open.await, Err(WasmError::InvocationTimeout(_))));
        assert_eq!(SLOTS.available_permits(), 0);
        assert!(matches!(
            run_wasi_preopen(
                workers,
                &SLOTS,
                Instant::now() + TEST_TIMEOUT,
                TEST_TIMEOUT,
                || { unreachable!("a second open must not start while the only slot is held") }
            )
            .await,
            Err(WasmError::ResourceLimitExceeded(WASI_PREOPEN_QUOTA)),
        ));

        #[cfg(actr_wasm_fixture_available)]
        {
            tokio::time::resume();
            let root = tempfile::tempdir().unwrap();
            let node = tokio::time::timeout(
                Duration::from_secs(20),
                crate::tests::attach_wasi_fixture(&root, None, crate::WasiDataDir::Inherit),
            )
            .await
            .expect("mount-free attach must not wait for the preopen worker");
            node.attachment
                .unwrap()
                .node
                .workload_dispatch
                .shutdown()
                .await;
            assert_eq!(SLOTS.available_permits(), 0);
        }

        drop(unblock);
        let _released = SLOTS.acquire().await.unwrap();
    });
}
