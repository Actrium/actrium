//! Local WASM integration harness. The fixture policy and in-process Actor
//! transport are restricted to synthetic data and are never used by Hyper.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use actr_config::WasiDataAccess;
use actr_pack::egress::{ActorPermission, HttpsPermission};
use actr_protocol::{Direction, RpcEnvelope};
use prost::Message;
use tokio::sync::Mutex;

use super::*;
use crate::config::WasiDataGrant;
use crate::wasm::WasmHost;

// cjk-allow: The fixture checks the Notebook's localized weather label.
const TOKYO_WEATHER_PREFIX: &str = "东京天气：";
// cjk-allow: The fixture checks the Notebook's localized forecast.
const TOKYO_RAIN_FORECAST: &str = "东京天气：雨天";
const PREFIX: &str = "actrium.notes_search.v1";
const INDEX: u64 = 2;
const WEATHER: u64 = 3;
const NOTEBOOK: u64 = 1;
const FRONT: u64 = 4;
const LIVE_URL: &str = "https://api.open-meteo.com/v1/forecast?latitude=35.6762&longitude=139.6503&daily=weather_code&timezone=Asia%2FTokyo&forecast_days=1";

fn id(serial_number: u64) -> ActrId {
    ActrId {
        realm: "fixture@notes.example".into(),
        serial_number,
    }
}

struct LocalPolicy {
    index: CancellationToken,
    weather_allowed: AtomicBool,
    weather_url: String,
}

impl EgressPolicy for LocalPolicy {
    fn authorize(&self, request: EgressRequest<'_>) -> ActorResult<EgressPermit> {
        let (target, revocation) = match (
            request.invocation.self_id.serial_number,
            request.alias,
            request.operation,
        ) {
            (NOTEBOOK, "index", EgressOperation::Actor { .. }) => {
                (Some(id(INDEX)), self.index.clone())
            }
            (NOTEBOOK, "weather", EgressOperation::Actor { .. }) => {
                (Some(id(WEATHER)), CancellationToken::new())
            }
            (
                WEATHER,
                "tokyo-weather",
                EgressOperation::Https {
                    method: "GET",
                    url,
                    required: false,
                },
            ) if self.weather_allowed.load(Ordering::Relaxed) && url == self.weather_url => {
                (None, CancellationToken::new())
            }
            _ => return Err(deny("fixture denied operation")),
        };
        Ok(EgressPermit {
            target,
            revocation,
            expires_at: Instant::now() + MAX_DURATION,
        })
    }

    fn authorize_inbound(
        &self,
        invocation: &InvocationContext,
        _: &str,
        _: &str,
        _: &str,
    ) -> ActorResult<()> {
        match (
            invocation.self_id.serial_number,
            invocation.caller_id.as_ref(),
        ) {
            (NOTEBOOK, Some(caller)) if *caller == id(FRONT) => Ok(()),
            (INDEX | WEATHER, Some(caller)) if *caller == id(NOTEBOOK) => Ok(()),
            _ => Err(deny("fixture rejected caller")),
        }
    }
}

async fn weather_fixture() -> (HttpsFixture, Arc<AtomicUsize>, Arc<AtomicBool>) {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let root = reqwest::Certificate::from_der(cert.cert.der()).unwrap();
    let key = rustls_pki_types::PrivateKeyDer::Pkcs8(cert.signing_key.serialize_der().into());
    let server = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert.cert.der().clone()], key)
        .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let redirect = Arc::new(AtomicBool::new(false));
    let observed = hits.clone();
    let redirecting = redirect.clone();
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let Ok(mut stream) = acceptor.accept(socket).await else {
                continue;
            };
            let mut request = Vec::new();
            while request.len() < 4096 && !request.ends_with(b"\r\n\r\n") {
                let mut buffer = [0; 1024];
                match stream.read(&mut buffer).await {
                    Ok(0) | Err(_) => break,
                    Ok(count) => request.extend_from_slice(&buffer[..count]),
                }
            }
            observed.fetch_add(1, Ordering::Relaxed);
            if !request.starts_with(b"GET /weather HTTP/1.1\r\n") {
                continue;
            }
            let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
            let tokyo = now.with_timezone(&chrono::FixedOffset::east_opt(9 * 3600).unwrap());
            let body = format!(
                r#"{{"timezone":"Asia/Tokyo","daily":{{"time":["{}"],"weather_code":[65]}}}}"#,
                tokyo.format("%Y-%m-%d")
            );
            let answer = if redirecting.load(Ordering::Relaxed) {
                "HTTP/1.1 302 Found\r\nLocation: /weather?redirected=1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned()
            } else {
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
            };
            let _ = stream.write_all(answer.as_bytes()).await;
            let _ = stream.shutdown().await;
        }
    });
    (HttpsFixture { address, root }, hits, redirect)
}

fn privacy_contract() -> PackagePrivacyContract {
    PackagePrivacyContract {
        sha256: "ab".repeat(32),
        egress: EgressPermissions::default(),
        dependency_digests: BTreeMap::new(),
    }
}

fn envelope(route: String, request_id: &str, payload: Vec<u8>) -> Vec<u8> {
    RpcEnvelope {
        route_key: route,
        request_id: request_id.into(),
        payload: Some(payload.into()),
        direction: Some(Direction::Request as i32),
        timeout_ms: 10_000,
        ..Default::default()
    }
    .encode_to_vec()
}

fn invocation(target: u64, caller: u64, request_id: &str) -> InvocationContext {
    InvocationContext {
        self_id: id(target),
        caller_id: Some(id(caller)),
        request_id: request_id.into(),
    }
}

#[derive(Clone, PartialEq, Message)]
struct SaveNote {
    #[prost(string, tag = "1")]
    command_id: String,
    #[prost(string, tag = "4")]
    title: String,
    #[prost(string, tag = "5")]
    body: String,
    #[prost(bool, tag = "6")]
    attach_tokyo_weather: bool,
}

#[derive(Clone, PartialEq, Message)]
struct SearchNotes {
    #[prost(string, tag = "1")]
    query: String,
    #[prost(uint32, tag = "2")]
    page_size: u32,
}

fn grant(directory: &Path) -> WasiDataGrant<'_> {
    WasiDataGrant {
        directory,
        access: WasiDataAccess::ReadWrite,
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "build the three Notes Search WASM components and set NOTES_SEARCH_WASM_DIR"]
async fn notebook_scoped_egress_wasm() {
    let dir = std::path::PathBuf::from(
        std::env::var_os("NOTES_SEARCH_WASM_DIR").expect("set NOTES_SEARCH_WASM_DIR"),
    );
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().canonicalize().unwrap();
    let notebook_dir = data.join("notebook");
    let index_dir = data.join("index");
    for path in [&notebook_dir, &index_dir] {
        std::fs::create_dir(path).unwrap();
    }
    let limits = crate::config::WasmRuntimeLimits {
        fuel_per_invocation: 100_000_000,
        invocation_timeout: Duration::from_secs(30),
        ..Default::default()
    };
    let open = |filename: &str| {
        WasmHost::compile_with_limits(&std::fs::read(dir.join(filename)).unwrap(), &limits).unwrap()
    };
    let live_weather = std::env::var_os("NOTES_SEARCH_LIVE_WEATHER").is_some();
    let (fixture, hits, redirect) = weather_fixture().await;
    let weather_url = if live_weather {
        LIVE_URL.to_owned()
    } else {
        format!("https://localhost:{}/weather", fixture.address.port())
    };
    let policy = Arc::new(LocalPolicy {
        index: CancellationToken::new(),
        weather_allowed: AtomicBool::new(false),
        weather_url: weather_url.clone(),
    });
    let mut weather_contract = privacy_contract();
    weather_contract.egress.https.insert(
        "tokyo-weather".into(),
        HttpsPermission {
            url: weather_url,
            method: "GET".into(),
            required: false,
            data_classes: vec!["public".into()],
        },
    );
    let mut weather_broker = EgressBroker::new(
        policy.clone(),
        "weather-fixture".into(),
        weather_contract,
        None,
    );
    if !live_weather {
        weather_broker.https_fixture = Some(fixture);
    }
    let index_host = open("index_core.wasm");
    let weather_host = open("weather_workload.wasm");
    let index = Arc::new(Mutex::new(
        index_host
            .instantiate_scoped(
                Some(grant(&index_dir)),
                Some(Arc::new(EgressBroker::new(
                    policy.clone(),
                    "index-fixture".into(),
                    privacy_contract(),
                    None,
                ))),
            )
            .await
            .unwrap(),
    ));
    let weather = Arc::new(Mutex::new(
        weather_host
            .instantiate_scoped(None, Some(Arc::new(weather_broker)))
            .await
            .unwrap(),
    ));

    let index_count = Arc::new(AtomicUsize::new(0));
    let weather_count = Arc::new(AtomicUsize::new(0));
    let no_outbound: HostAbiFn = Arc::new(|_| {
        Box::pin(async { HostOperationResult::Error(deny("fixture leaf asked for legacy RPC")) })
    });
    let bridge: HostAbiFn = {
        let index_count = index_count.clone();
        let weather_count = weather_count.clone();
        Arc::new(move |operation| {
            let (index, weather) = (index.clone(), weather.clone());
            let (index_count, weather_count) = (index_count.clone(), weather_count.clone());
            let no_outbound = no_outbound.clone();
            Box::pin(async move {
                let HostOperation::ScopedActorCall { request: call, .. } = operation else {
                    return HostOperationResult::Error(deny("unexpected fixture operation"));
                };
                let (destination, counter) = if call.target == id(INDEX) {
                    (index, index_count)
                } else if call.target == id(WEATHER) {
                    (weather, weather_count)
                } else {
                    return HostOperationResult::Error(deny("unknown fixture target"));
                };
                let serial = call.target.serial_number;
                let request = format!(
                    "fixture-{}-{}",
                    serial,
                    counter.fetch_add(1, Ordering::Relaxed)
                );
                let input = envelope(call.route_key, &request, call.payload);
                let remote = tokio::spawn(async move {
                    destination
                        .lock()
                        .await
                        .handle(&input, invocation(serial, NOTEBOOK, &request), &no_outbound)
                        .await
                });
                match remote.await {
                    Ok(Ok(bytes)) => HostOperationResult::Bytes(bytes),
                    Ok(Err(error)) => {
                        HostOperationResult::Error(ActrError::Unavailable(error.to_string()))
                    }
                    Err(error) => {
                        HostOperationResult::Error(ActrError::Unavailable(error.to_string()))
                    }
                }
            })
        })
    };
    let mut notebook_contract = privacy_contract();
    for (alias, service, methods) in [
        (
            "index",
            "IndexWorkerService",
            &["SubmitMutation", "GetTask", "SearchIndex"][..],
        ),
        ("weather", "WeatherService", &["GetTokyoWeather"][..]),
    ] {
        notebook_contract.egress.actor.insert(
            alias.into(),
            ActorPermission {
                rpc_routes: methods
                    .iter()
                    .map(|method| format!("{PREFIX}.{service}.{method}"))
                    .collect(),
                data_classes: vec![
                    if alias == "index" {
                        "private"
                    } else {
                        "public"
                    }
                    .into(),
                ],
                target_actor_type: format!("notes.example/{alias}@1.0.0"),
            },
        );
    }
    let mut lock = actr_config::lock::LockFile::new();
    for alias in ["index", "weather"] {
        lock.dependencies.push(actr_config::lock::LockedDependency {
            name: alias.into(),
            alias: alias.into(),
            actr_type: format!("notes.example/{alias}@1.0.0"),
            package_sha256: Some("a".repeat(64)),
            description: None,
            fingerprint: "service_semantic:fixture".into(),
            published_at: None,
            tags: vec![],
            cached_at: "2026-11-19T00:00:00Z".into(),
            files: vec![],
        });
    }
    let notebook_host = open("application_core.wasm");
    let mut notebook = notebook_host
        .instantiate_scoped(
            Some(grant(&notebook_dir)),
            Some(Arc::new(EgressBroker::new(
                policy.clone(),
                "notebook-fixture".into(),
                notebook_contract,
                Some(&lock),
            ))),
        )
        .await
        .unwrap();

    let note = SaveNote {
        command_id: "command-one".into(),
        title: "Synthetic".into(),
        body: "synthetic-needle".into(),
        attach_tokyo_weather: true,
    };
    let save = envelope(
        format!("{PREFIX}.NotebookService.SaveNote"),
        "save",
        note.encode_to_vec(),
    );
    assert!(
        notebook
            .handle(&save, invocation(NOTEBOOK, INDEX, "save"), &bridge)
            .await
            .is_err()
    );
    assert_eq!(index_count.load(Ordering::Relaxed), 0);
    assert_eq!(weather_count.load(Ordering::Relaxed), 0);
    let result = notebook
        .handle(&save, invocation(NOTEBOOK, FRONT, "save"), &bridge)
        .await
        .unwrap();
    assert_eq!(weather_count.load(Ordering::Relaxed), 1);
    assert_eq!(index_count.load(Ordering::Relaxed), 2);
    assert!(contains(&result, "synthetic-needle"));
    assert!(!contains(&result, TOKYO_WEATHER_PREFIX));
    assert_eq!(hits.load(Ordering::Relaxed), 0);
    notebook
        .handle(&save, invocation(NOTEBOOK, FRONT, "retry"), &bridge)
        .await
        .unwrap();
    assert_eq!(weather_count.load(Ordering::Relaxed), 1);
    policy.weather_allowed.store(true, Ordering::Relaxed);
    let mut note_with_weather = note.clone();
    note_with_weather.command_id = "command-with-weather".into();
    let weather_save = envelope(
        format!("{PREFIX}.NotebookService.SaveNote"),
        "weather-save",
        note_with_weather.encode_to_vec(),
    );
    let result = notebook
        .handle(
            &weather_save,
            invocation(NOTEBOOK, FRONT, "weather-save"),
            &bridge,
        )
        .await
        .unwrap();
    assert!(
        contains(&result, TOKYO_WEATHER_PREFIX),
        "approved forecast was not attached"
    );
    if !live_weather {
        assert!(contains(&result, TOKYO_RAIN_FORECAST));
        assert_eq!(hits.load(Ordering::Relaxed), 1);
        redirect.store(true, Ordering::Relaxed);
        note_with_weather.command_id = "command-with-redirect".into();
        let redirect_save = envelope(
            format!("{PREFIX}.NotebookService.SaveNote"),
            "redirect-save",
            note_with_weather.encode_to_vec(),
        );
        let result = notebook
            .handle(
                &redirect_save,
                invocation(NOTEBOOK, FRONT, "redirect-save"),
                &bridge,
            )
            .await
            .unwrap();
        assert!(contains(&result, "synthetic-needle"));
        assert!(!contains(&result, TOKYO_WEATHER_PREFIX));
        assert_eq!(hits.load(Ordering::Relaxed), 2);
    }

    let search = envelope(
        format!("{PREFIX}.NotebookService.SearchNotes"),
        "search",
        SearchNotes {
            query: "synthetic-needle".into(),
            page_size: 10,
        }
        .encode_to_vec(),
    );
    let result = notebook
        .handle(&search, invocation(NOTEBOOK, FRONT, "search"), &bridge)
        .await
        .unwrap();
    assert!(contains(&result, "synthetic-needle"));

    let mut timeout_contract = privacy_contract();
    timeout_contract.egress.actor.insert(
        "index".into(),
        ActorPermission {
            rpc_routes: vec![format!("{PREFIX}.IndexWorkerService.SearchIndex")],
            data_classes: vec!["private".into()],
            target_actor_type: "notes.example/index@1.0.0".into(),
        },
    );
    let timeout_limits = crate::config::WasmRuntimeLimits {
        invocation_timeout: Duration::from_secs(1),
        ..limits
    };
    let timeout_host = WasmHost::compile_with_limits(
        &std::fs::read(dir.join("application_core.wasm")).unwrap(),
        &timeout_limits,
    )
    .unwrap();
    let mut timeout_notebook = timeout_host
        .instantiate_scoped(
            Some(grant(&notebook_dir)),
            Some(Arc::new(EgressBroker::new(
                policy.clone(),
                "notebook-fixture".into(),
                timeout_contract,
                Some(&lock),
            ))),
        )
        .await
        .unwrap();
    let delayed: HostAbiFn = Arc::new(|operation| {
        Box::pin(async move {
            let HostOperation::ScopedActorCall { dispatch_id, .. } = operation else {
                panic!("unexpected fixture operation");
            };
            dispatch_id.set("delayed-index-operation".into()).unwrap();
            std::future::pending().await
        })
    });
    let scope = RequestScope::root_at(Instant::now() + Duration::from_secs(10));
    let error = timeout_notebook
        .handle_with_reply(
            &search,
            invocation(NOTEBOOK, FRONT, "timeout"),
            &delayed,
            None,
            true,
            None,
            None,
            Some(scope),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(error, crate::wasm::WasmError::DispatchFailed(ActrError::DeliveryUncertain { operation_id, .. }) if operation_id == "delayed-index-operation")
    );

    let count = index_count.load(Ordering::Relaxed);
    policy.index.cancel();
    assert!(
        notebook
            .handle(&search, invocation(NOTEBOOK, FRONT, "revoked"), &bridge)
            .await
            .is_err()
    );
    assert_eq!(index_count.load(Ordering::Relaxed), count);
}

fn contains(bytes: &[u8], needle: &str) -> bool {
    bytes
        .windows(needle.len())
        .any(|bytes| bytes == needle.as_bytes())
}
