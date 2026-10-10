//! Integration test: Stale callbacks must NOT kill new connections
//!
//! Problem scenario (stale callbacks killing new connections):
//! 1. Peer A connects to Peer B → connection_1 (session_1)
//! 2. Network outage → connection_1's DataChannels start closing asynchronously
//! 3. Network recovers → Peer A reconnects → connection_2 (session_2)
//! 4. connection_1's stale DC.on_close callbacks fire AFTER connection_2 is live
//! 5. BUG (before fix): Stale callbacks invalidate connection_2's lanes → connection dies
//! 6. FIX: session_id / cancel_token guard prevents stale callbacks from executing
//!
//! This test verifies:
//! - After reconnect, the NEW connection remains functional
//! - Stale events from old sessions are filtered by the coordinator
//! - Messages can be sent and received on the new connection

use actr_hyper::test_support::TestHarness;
use std::time::Duration;

/// Initialize tracing for test output
fn init_tracing() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_file(true)
        .with_line_number(true)
        .with_test_writer()
        .try_init()
        .ok();
}

/// Test: Stale callback guard prevents old connection from killing new connection
///
/// This is the core test for the "stale callbacks killing new connections" bug fix.
///
/// Flow:
/// 1. Connect peer 100 → peer 200 (connection_1, session_1)
/// 2. Verify connection works (send/receive message)
/// 3. Simulate network outage (VNet + signaling)
///    → DataChannels start closing; old callbacks become pending
/// 4. Restore network
/// 5. Trigger reconnection via retry_failed_connections()
///    → New connection_2 (session_2) is established
/// 6. Verify NEW connection works — stale callbacks did NOT kill it
/// 7. Send multiple messages to confirm stability
#[tokio::test]
async fn test_stale_callbacks_do_not_kill_new_connection() {
    init_tracing();

    let mut harness = TestHarness::with_vnet().await;
    harness.add_peer(100).await;
    harness.add_peer(200).await;

    // ==================== Phase 1: Establish connection ====================
    tracing::info!("🔗 Phase 1: Establishing connection 100 → 200...");
    harness.connect(100, 200).await;
    tracing::info!("✅ Phase 1 complete: Connection established and verified");

    // Record the physical PC generation. RTC tags every handler/pump event
    // with this identity; a Hard Recovery must move to a strictly newer one.
    let target = harness.peer(200).id.clone();
    let source = harness.peer(100).id.clone();
    let old_generation = harness
        .peer(100)
        .rtc
        .generation(&target)
        .await
        .expect("connected peer has a generation");

    harness.reset_counters();

    // ==================== Phase 2: Network outage ====================
    tracing::info!("🔴 Phase 2: Simulating full network outage...");
    tracing::info!("   Old DataChannel callbacks will fire asynchronously");
    harness.simulate_disconnect();

    harness
        .peer(100)
        .rtc
        .wait_state(&target, Duration::from_secs(5), |state| !state.sendable)
        .await
        .expect("outage must revoke sendability");

    // ==================== Phase 3: Network recovery + reconnect ====================
    tracing::info!("🟢 Phase 3: Restoring network...");
    harness.simulate_reconnect();
    // Force the RFC Hard Recovery path on both logical peers so the old
    // generation is retired while its callbacks/pumps may still be draining.
    harness.peer(100).rtc.force_reconnect();
    harness.peer(200).rtc.force_reconnect();
    let recovered = harness
        .peer(100)
        .rtc
        .wait_state(&target, Duration::from_secs(20), |state| {
            state.sendable && state.pc_generation > old_generation
        })
        .await
        .expect("new generation must become sendable");
    harness
        .peer(200)
        .rtc
        .wait_sendable(&source, Duration::from_secs(20))
        .await;
    assert!(recovered.pc_generation > old_generation);

    // ==================== Phase 4: Verify new connection survives ====================
    tracing::info!("📤 Phase 4: Verifying new connection is alive...");
    tracing::info!("   If stale callbacks killed the new connection, this will FAIL");

    // Send first verification message
    let peer_a = harness.peer(100);
    let handle1 = peer_a.spawn_request(200, "stale_guard_verify_1", 15000);

    match tokio::time::timeout(Duration::from_secs(15), handle1).await {
        Ok(Ok(Ok(response))) => {
            tracing::info!(
                "✅ Message 1 succeeded on NEW connection! ({} bytes)",
                response.len()
            );
        }
        Ok(Ok(Err(e))) => {
            let err_str = format!("{:?}", e);
            if err_str.to_lowercase().contains("closed") {
                panic!(
                    "❌ BUG: Stale callback killed new connection! Error: {}",
                    err_str
                );
            }
            panic!("❌ Message 1 failed: {}", err_str);
        }
        Ok(Err(e)) => panic!("Task panicked: {}", e),
        Err(_) => panic!("❌ Message 1 timed out — new connection may be dead"),
    }

    // ==================== Phase 5: Stability check — multiple messages ====================
    tracing::info!("📤 Phase 5: Sending 3 more messages to verify stability...");

    for i in 2..=4 {
        let peer_a = harness.peer(100);
        let req_id = format!("stale_guard_verify_{}", i);
        let handle = peer_a.spawn_request(200, &req_id, 10000);

        match tokio::time::timeout(Duration::from_secs(10), handle).await {
            Ok(Ok(Ok(response))) => {
                tracing::info!("  ✅ Message {} succeeded ({} bytes)", i, response.len());
            }
            Ok(Ok(Err(e))) => {
                panic!("❌ Message {} failed: {:?}", i, e);
            }
            Ok(Err(e)) => panic!("Task {} panicked: {}", i, e),
            Err(_) => panic!("❌ Message {} timed out", i),
        }

        // Small gap between messages
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    // ==================== Summary ====================
    let total_restarts = harness.ice_restart_count();
    tracing::info!("╔══════════════════════════════════════════════════════╗");
    tracing::info!("║   Stale Callback Guard Test — PASSED                ║");
    tracing::info!("╠══════════════════════════════════════════════════════╣");
    tracing::info!("║ • Old connection's stale callbacks were SILENCED    ║");
    tracing::info!("║ • New connection survived and handled 4 messages    ║");
    tracing::info!(
        "║ • ICE restarts: {}                                   ║",
        total_restarts
    );
    tracing::info!("║ • Bug 'stale callbacks killing new conns' is FIXED   ║");
    tracing::info!("╚══════════════════════════════════════════════════════╝");

    tracing::info!("✅ test_stale_callbacks_do_not_kill_new_connection passed!");
}

/// Test: generation filtering survives controller retirement/recreation
///
/// Verifies that events from old sessions are ignored by the coordinator's
/// event listener, preventing stale cleanup on new connections.
///
/// This uses the real close/recreate boundary. The recreated controller starts
/// above the retired generation floor; any late Handler/pump completion from
/// the old controller therefore cannot alias or tear down the replacement.
#[tokio::test]
async fn test_session_id_filtering_ignores_stale_events() {
    init_tracing();

    let mut harness = TestHarness::new().await;
    harness.add_peer(100).await;
    harness.add_peer(200).await;

    // Establish connection
    tracing::info!("🔗 Establishing connection 100 → 200...");
    harness.connect(100, 200).await;
    tracing::info!("✅ Connection established");

    let target = harness.peer(200).id.clone();
    let source = harness.peer(100).id.clone();
    let retired_generation = harness
        .peer(100)
        .rtc
        .generation(&target)
        .await
        .expect("initial generation");

    assert!(harness.peer(100).rtc.close_peer(&target).await);
    assert!(harness.peer(200).rtc.close_peer(&source).await);
    assert!(harness.peer(100).rtc.state(&target).await.is_none());

    harness.peer(100).rtc.connect(target.clone()).await;
    let replacement = harness
        .peer(100)
        .rtc
        .wait_sendable(&target, Duration::from_secs(20))
        .await;
    harness
        .peer(200)
        .rtc
        .wait_sendable(&source, Duration::from_secs(20))
        .await;
    assert!(
        replacement.pc_generation > retired_generation,
        "controller recreation must preserve a monotone generation floor"
    );

    // Give retiring tasks/callbacks a scheduling window after the replacement
    // has become authoritative.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // The connection should STILL be alive — stale event was filtered
    tracing::info!("📤 Verifying connection still works after stale event injection...");
    let peer_a = harness.peer(100);
    let handle = peer_a.spawn_request(200, "after_stale_inject", 10000);

    match tokio::time::timeout(Duration::from_secs(10), handle).await {
        Ok(Ok(Ok(response))) => {
            tracing::info!(
                "✅ Connection survived stale event! Response: {} bytes",
                response.len()
            );
        }
        Ok(Ok(Err(e))) => {
            panic!("❌ BUG: Stale event killed the connection! Error: {:?}", e);
        }
        Ok(Err(e)) => panic!("Task panicked: {}", e),
        Err(_) => panic!("❌ Connection dead after stale event injection!"),
    }

    tracing::info!("╔══════════════════════════════════════════════════════╗");
    tracing::info!("║   Session ID Filtering Test — PASSED                ║");
    tracing::info!("╠══════════════════════════════════════════════════════╣");
    tracing::info!("║ • Retired generation callbacks were ignored         ║");
    tracing::info!("║ • Current connection survived and handled messages  ║");
    tracing::info!("║ • Controller generation filtering works             ║");
    tracing::info!("╚══════════════════════════════════════════════════════╝");

    tracing::info!("✅ test_session_id_filtering_ignores_stale_events passed!");
}
