//! Full linked-workload fixture. No test echo responder or PeerGate replacement.
#![allow(dead_code)] // Shared by independent integration-test binaries.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use actr_framework::{Bytes, Context, MessageDispatcher, Workload};
use actr_hyper::lifecycle::{AppLifecycleState, NetworkEventHandle};
use actr_hyper::test_support::{
    LoranTestServices, TestRtcControl, TestRtcRole, make_actor_id, make_actor_type,
};
use actr_hyper::{ActrRef, Attached, Hyper, HyperConfig, Node, StaticTrust};
use actr_protocol::{ActorResult, DataChunk, PayloadType, RpcEnvelope, register_response};
use async_trait::async_trait;
use tempfile::TempDir;
use tokio::sync::{Notify, Semaphore};

pub const WAIT: Duration = Duration::from_secs(20);

/// Start the production node with a fixture registration result. Only the Issuer
/// issuance step is supplied by the test; startup, credentials, lifecycle,
/// PeerGate, Mailbox, workload dispatch and response delivery remain real.
async fn start_node_with_fixture_registration(
    mut node: Node<Attached>,
    registration: register_response::RegisterOk,
) -> ActorResult<ActrRef> {
    let mut attachment = node.attachment.take().expect("attached fixture node");
    attachment.node.set_preregistered_credential(registration);
    let signaling = attachment.node.signaling_client.clone();
    let node = attachment.node.start().await?;
    signaling
        .probe_alive(Duration::from_secs(5))
        .await
        .map_err(actr_protocol::ActrError::from)?;
    Ok(node)
}

pub struct Journal {
    entries: Mutex<Vec<(String, Bytes)>>,
    changed: Notify,
    pub reply_release: Semaphore,
    pub chunk_release: Semaphore,
}

impl Default for Journal {
    fn default() -> Self {
        Self {
            entries: Mutex::default(),
            changed: Notify::new(),
            reply_release: Semaphore::new(0),
            chunk_release: Semaphore::new(0),
        }
    }
}

impl Journal {
    pub fn record(&self, key: impl Into<String>, payload: Bytes) {
        self.entries.lock().unwrap().push((key.into(), payload));
        self.changed.notify_waiters();
    }

    pub fn count(&self, key: &str) -> usize {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .filter(|(k, _)| k == key)
            .count()
    }

    pub fn payloads(&self, key: &str) -> Vec<Bytes> {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .collect()
    }

    pub async fn wait(&self, key: &str, count: usize) {
        tokio::time::timeout(WAIT, async {
            loop {
                let notify = self.changed.notified();
                tokio::pin!(notify);
                notify.as_mut().enable();
                if self.count(key) >= count {
                    return;
                }
                notify.await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("missing production event {key}, want {count}"));
    }
}

pub struct RecordingWorkload(pub Arc<Journal>);
pub struct RecordingDispatcher;

#[async_trait]
impl Workload for RecordingWorkload {
    type Dispatcher = RecordingDispatcher;
}

#[async_trait]
impl MessageDispatcher for RecordingDispatcher {
    type Workload = RecordingWorkload;
    async fn dispatch<C: Context>(
        w: &Self::Workload,
        e: RpcEnvelope,
        ctx: &C,
    ) -> ActorResult<Bytes> {
        let payload = e.payload.clone().unwrap_or_default();
        w.0.record(e.route_key.clone(), payload.clone());
        match e.route_key.as_str() {
            "hold" => {
                w.0.reply_release.acquire().await.unwrap().forget();
            }
            "open-stream" => {
                let stream = String::from_utf8(payload.to_vec()).unwrap();
                let journal = w.0.clone();
                ctx.register_stream(stream.clone(), move |chunk, _| {
                    let journal = journal.clone();
                    let stream = stream.clone();
                    Box::pin(async move {
                        journal.record(
                            format!("chunk-enter:{stream}:{}", chunk.sequence),
                            chunk.payload.clone(),
                        );
                        if stream.starts_with("held-") {
                            journal.chunk_release.acquire().await.unwrap().forget();
                        }
                        journal.record(format!("chunk:{stream}:{}", chunk.sequence), chunk.payload);
                        Ok(())
                    })
                })
                .await?;
            }
            "finish" => {
                let stream = String::from_utf8(payload.to_vec()).unwrap();
                ctx.unregister_stream(&stream).await?;
                w.0.record(format!("finished:{stream}"), Bytes::new());
            }
            _ => {}
        }
        Ok(payload)
    }
}

pub struct Nodes {
    pub nodes: [ActrRef; 2],
    pub events: [NetworkEventHandle; 2],
    pub rtc: [TestRtcControl; 2],
    pub journals: [Arc<Journal>; 2],
    pub services: LoranTestServices,
    _dirs: [TempDir; 2],
}

impl Nodes {
    pub async fn new(mobile_role: TestRtcRole) -> Self {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "warn".into()),
            )
            .with_test_writer()
            .try_init();
        let services = LoranTestServices::start().await.unwrap();
        let dirs = [TempDir::new().unwrap(), TempDir::new().unwrap()];
        let journals = [Arc::new(Journal::default()), Arc::new(Journal::default())];
        let serials = match mobile_role {
            TestRtcRole::Offerer => [62_020, 62_010],
            TestRtcRole::Answerer => [62_010, 62_020],
            _ => unreachable!(),
        };
        let mut started = Vec::new();
        let mut events = Vec::new();
        for i in 0..2 {
            let id = make_actor_id(serials[i]);
            let hyper = Hyper::new(HyperConfig::new(
                dirs[i].path(),
                Arc::new(StaticTrust::dev_only()),
            ))
            .await
            .unwrap();
            let config = actr_config::RuntimeConfig {
                package: actr_config::PackageInfo {
                    name: format!("governance-{i}"),
                    actr_type: make_actor_type(),
                    description: None,
                    authors: vec![],
                    license: None,
                },
                signaling_url: services.signaling_url().parse().unwrap(),
                realm: id.realm.clone(),
                issuer_endpoint: "http://127.0.0.1:1/issuer".into(),
                realm_client_key: None,
                visible_in_discovery: true,
                acl: None,
                mailbox_path: None,
                scripts: HashMap::new(),
                webrtc: Default::default(),
                observability: actr_config::ObservabilityConfig {
                    filter_level: "warn".into(),
                    tracing_enabled: false,
                    tracing_endpoint: String::new(),
                    tracing_service_name: "governance-fixture".into(),
                    logs_enabled: false,
                    ca_cert_path: None,
                },
                config_dir: dirs[i].path().into(),
                trust: vec![],
                package_path: None,
                web: None,
                storage_root: None,
            };
            let mut node = Node::from_hyper(hyper, config)
                .link(RecordingWorkload(journals[i].clone()))
                .await
                .unwrap();
            let event = node.create_gated_network_event_handle(0);
            let turn = services.turn_credential(&id);
            let expires = std::time::UNIX_EPOCH + Duration::from_secs(turn.expires_at);
            let registration = register_response::RegisterOk {
                credential: services.aid_credential(&id),
                actr_id: id,
                turn_credential: turn,
                credential_expires_at: Some(expires.into()),
                signaling_heartbeat_interval_secs: 3600,
                ..Default::default()
            };
            let running = tokio::time::timeout(
                WAIT,
                start_node_with_fixture_registration(node, registration),
            )
            .await
            .expect("node startup deadline")
            .expect("production node startup");
            event
                .handle_app_lifecycle_changed(AppLifecycleState::Foreground {
                    background_duration_ms: 0,
                })
                .await
                .unwrap();
            events.push(event);
            started.push(running);
        }
        let nodes: [ActrRef; 2] = started.try_into().ok().unwrap();
        let rtc = [
            TestRtcControl::for_node(&nodes[0]),
            TestRtcControl::for_node(&nodes[1]),
        ];
        let pair = Self {
            nodes,
            events: events.try_into().ok().unwrap(),
            rtc,
            journals,
            services,
            _dirs: dirs,
        };
        // Startup awaits a signaling Ping/Pong registration barrier. These
        // linked fixture nodes have no published ServiceSpec, so discovery
        // eligibility is not a valid readiness oracle for this transport test.
        pair.echo(0, "baseline").await;
        pair.echo(1, "baseline").await;
        assert_eq!(
            pair.rtc[0].role(&pair.nodes[1].actor_id()).await,
            Some(mobile_role)
        );
        pair
    }

    pub async fn call(
        &self,
        from: usize,
        route: &str,
        payload: Bytes,
        timeout_ms: i64,
    ) -> ActorResult<Bytes> {
        self.nodes[from]
            .app_context()
            .await
            .call_raw(
                &actr_framework::Dest::Peer(self.nodes[1 - from].actor_id()),
                route.into(),
                PayloadType::RpcReliable,
                payload,
                timeout_ms,
            )
            .await
    }

    pub async fn echo(&self, from: usize, label: &str) {
        let payload = Bytes::from(label.to_owned());
        let result = self
            .call(from, "echo", payload.clone(), 10_000)
            .await
            .expect("production RPC round trip");
        assert_eq!(result, payload);
    }

    pub async fn background(&self, i: usize) {
        self.events[i]
            .handle_app_lifecycle_changed(AppLifecycleState::Background)
            .await
            .unwrap();
        self.rtc[i]
            .wait_state(&self.nodes[1 - i].actor_id(), WAIT, |s| !s.sendable)
            .await
            .expect("background revoke");
    }

    pub async fn foreground(&self, i: usize, millis: u64) {
        self.events[i]
            .handle_app_lifecycle_changed(AppLifecycleState::Foreground {
                background_duration_ms: millis,
            })
            .await
            .unwrap();
    }

    pub async fn ready(&self) -> [actr_framework::WebRtcPeerState; 2] {
        let a_id = self.nodes[0].actor_id();
        let b_id = self.nodes[1].actor_id();
        let (a, b) = tokio::join!(
            self.rtc[0].wait_sendable(&b_id, WAIT),
            self.rtc[1].wait_sendable(&a_id, WAIT)
        );
        [a, b]
    }

    pub async fn stream(&self, from: usize, stream: &str, sequence: u64, lane: PayloadType) {
        let payload = Bytes::from(vec![sequence as u8; 32 * 1024]);
        self.nodes[from]
            .app_context()
            .await
            .send_data_chunk_with_type(
                &actr_framework::Dest::Peer(self.nodes[1 - from].actor_id()),
                lane,
                DataChunk {
                    stream_id: stream.into(),
                    sequence,
                    payload: payload.clone(),
                    ..Default::default()
                },
            )
            .await
            .expect("production chunk submission");
        let key = format!("chunk:{stream}:{sequence}");
        self.journals[1 - from].wait(&key, 1).await;
        assert_eq!(self.journals[1 - from].payloads(&key), vec![payload]);
    }

    pub async fn shutdown(self) {
        for n in &self.nodes {
            n.shutdown();
        }
        tokio::time::timeout(Duration::from_secs(35), async {
            tokio::join!(
                self.nodes[0].wait_for_shutdown(),
                self.nodes[1].wait_for_shutdown()
            );
        })
        .await
        .expect("production node shutdown");
        self.services.shutdown().await;
    }
}
