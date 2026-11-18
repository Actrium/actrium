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

#[tokio::test(start_paused = true)]
async fn mounted_resident_deadlines_and_host_operations_use_caller_runtime() {
    let caller = Handle::current();
    let guest = wasi_guest_runtime().unwrap();
    let caller_id = caller.id();
    let (deadline_tx, mut deadline_rx) = mpsc::unbounded_channel();
    let mut state = HostState::new(&WasmRuntimeLimits::default());
    state.set_wasi_runtime_isolation(true);
    let host_abi: HostAbiFn = Arc::new(move |_| {
        assert_eq!(Handle::current().id(), caller_id);
        Box::pin(async move {
            assert_eq!(Handle::current().id(), caller_id);
            HostOperationResult::Done
        })
    });
    let ctx = InvocationContext {
        self_id: ActrId::default(),
        caller_id: None,
        request_id: "runtime-context".into(),
    };
    let guest_for_call = guest.clone();
    with_wasi_runtime(Some(guest), async {
        with_wasi_runtime(Some(caller), async {
            let _deadline = arm_deadline(1, 2, TEST_TIMEOUT, &deadline_tx);
            let token = state.alloc_invocation(ctx, host_abi);
            let host = state.invocation_host_abi(token);
            with_wasi_runtime(Some(guest_for_call), async {
                run_host_operation(host, HostOperation::Discover(HostDiscoverV1::default()))
                    .await
                    .unwrap()
                    .unwrap();
            })
            .await;
            tokio::task::yield_now().await;
            advance(TEST_TIMEOUT + Duration::from_millis(1)).await;
            let expired = deadline_rx.recv().await.unwrap();
            assert_eq!((expired.generation, expired.token), (1, 2));
        })
        .await
    })
    .await;
}

#[tokio::test]
async fn canceled_host_operation_drops_on_caller_runtime() {
    struct PendingHostOperation {
        caller: tokio::runtime::Id,
        dropped: Option<oneshot::Sender<bool>>,
    }
    impl std::future::Future for PendingHostOperation {
        type Output = HostOperationResult;
        fn poll(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            std::task::Poll::Pending
        }
    }
    impl Drop for PendingHostOperation {
        fn drop(&mut self) {
            let on_caller = Handle::current().id() == self.caller;
            let _ = self.dropped.take().unwrap().send(on_caller);
        }
    }

    let caller = Handle::current().id();
    let guest = wasi_guest_runtime().unwrap();
    let (dropped_tx, dropped_rx) = oneshot::channel();
    let dropped_tx = Arc::new(Mutex::new(Some(dropped_tx)));
    let host_abi: HostAbiFn = Arc::new(move |_| {
        Box::pin(PendingHostOperation {
            caller,
            dropped: dropped_tx.lock().unwrap().take(),
        })
    });
    let mut state = HostState::new(&WasmRuntimeLimits::default());
    state.set_wasi_runtime_isolation(true);
    let token = state.alloc_invocation(
        InvocationContext {
            self_id: ActrId::default(),
            caller_id: None,
            request_id: "cancel-runtime".into(),
        },
        host_abi,
    );
    let host_abi = state.invocation_host_abi(token).unwrap();
    with_wasi_runtime(Some(guest), async move {
        let mut future = host_abi(HostOperation::Discover(HostDiscoverV1::default()));
        assert!(futures_util::poll!(future.as_mut()).is_pending());
        drop(future);
    })
    .await;
    assert!(dropped_rx.await.unwrap());
}
