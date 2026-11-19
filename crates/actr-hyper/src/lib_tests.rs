use super::*;
#[cfg(all(feature = "wasm-engine", actr_wasm_fixture_available))]
use actr_config::WasiDataAccess::{self, ReadOnly, ReadWrite};
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
#[cfg(feature = "dynclib-engine")]
use std::sync::{Arc, Barrier};
use tempfile::TempDir;

#[cfg(all(feature = "wasm-engine", actr_wasm_fixture_available))]
pub(crate) async fn attach_wasi_fixture(
    root: &TempDir,
    deployment: Option<(PathBuf, WasiDataAccess)>,
    grant: WasiDataDir,
    requested: Option<WasiDataAccess>,
) -> HyperResult<Node<Attached>> {
    use actr_pack::{ActorPackOptions, ActorPackageManifest};
    use std::collections::BTreeMap;

    let key = SigningKey::generate(&mut OsRng);
    let mut manifest = ActorPackageManifest::from_toml(include_str!(
        "../../actr-pack/tests/fixtures/registration-v2.toml"
    ))
    .unwrap();
    if let Some(access) = requested {
        let capability = match access {
            ReadOnly => package_runtime::WASI_DATA_READ_ONLY,
            ReadWrite => package_runtime::WASI_DATA_READ_WRITE,
        };
        manifest.host.required_capabilities.push(capability.into());
    }
    let package = WorkloadPackage::new(
        actr_pack::pack_actor_package(&ActorPackOptions {
            manifest,
            payloads: BTreeMap::from([(
                "workload/echo.wasm".into(),
                include_bytes!(env!("ACTR_WASM_FIXTURE")).to_vec(),
            )]),
            signing_key: key.clone(),
        })
        .unwrap(),
    );
    let trust = StaticTrust::new(key.verifying_key().to_bytes()).unwrap();
    let mut hyper_config = HyperConfig::new(root.path().join("hyper"), Arc::new(trust));
    hyper_config.wasi_data_dir = grant;
    let hyper = Hyper::new(hyper_config).await.unwrap();
    let mut config = linked_runtime_config(root);
    if let Some((path, access)) = deployment {
        config.wasi_data_dir = Some(path);
        config.wasi_data_access = access;
    }
    Node::from_hyper(hyper, config).attach(&package).await
}

#[cfg(all(feature = "wasm-engine", actr_wasm_fixture_available))]
async fn call_wasi_fixture(
    actor: &crate::executor::ActorHandle,
    route: &str,
    payload: &'static [u8],
) -> actr_protocol::ActorResult<bytes::Bytes> {
    use crate::workload::{HostAbiFn, HostOperationResult, InvocationContext};
    use actr_protocol::{ActrId, Direction, RpcEnvelope};

    let ctx = crate::test_support::runtime_context_with_host_transport(
        ActrId::default(),
        Arc::new(crate::transport::HostTransport::new()),
    );
    let host_abi: HostAbiFn = Arc::new(|_| Box::pin(async { HostOperationResult::Done }));
    actor
        .dispatch_envelope(
            RpcEnvelope {
                route_key: route.into(),
                request_id: route.into(),
                direction: Some(Direction::Request as i32),
                payload: Some(bytes::Bytes::from_static(payload)),
                ..Default::default()
            },
            ctx,
            InvocationContext {
                self_id: ActrId::default(),
                caller_id: None,
                request_id: route.into(),
            },
            &host_abi,
        )
        .await
}

#[cfg(all(unix, feature = "wasm-engine", actr_wasm_fixture_available))]
#[test]
fn stalled_guest_file_io_does_not_block_mount_free_attach() {
    use std::os::unix::fs::OpenOptionsExt;
    use std::time::Duration;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    runtime.block_on(async {
        let mounted_root = TempDir::new().unwrap();
        let other_root = TempDir::new().unwrap();
        let data = mounted_root.path().join("data");
        std::fs::create_dir(&data).unwrap();
        let fifo = data.join("fixture-state");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        let mounted = attach_wasi_fixture(
            &mounted_root,
            Some((data.clone(), ReadWrite)),
            WasiDataDir::Inherit,
            Some(ReadWrite),
        )
        .await
        .unwrap();
        let actor = mounted.attachment.unwrap().node.workload_dispatch;
        let request_actor = actor.clone();
        let mut request = tokio::spawn(async move {
            call_wasi_fixture(&request_actor, "test/wasi-data-block", b"").await
        });
        let marker = data.join("ready");
        let ready = tokio::time::timeout(Duration::from_secs(20), async {
            while !marker.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        // Give the guest time to issue the FIFO open after creating the marker.
        let blocked = tokio::time::timeout(Duration::from_millis(200), &mut request)
            .await
            .is_err();
        let timed_out = if blocked {
            match tokio::time::timeout(Duration::from_secs(10), &mut request).await {
                Ok(Ok(Err(actr_protocol::ActrError::TimedOut))) => true,
                Ok(Ok(Err(actr_protocol::ActrError::Internal(message)))) => {
                    message.contains("WASM invocation timed out")
                }
                _ => false,
            }
        } else {
            false
        };
        let attached = tokio::time::timeout(
            Duration::from_secs(20),
            attach_wasi_fixture(&other_root, None, WasiDataDir::Inherit, None),
        )
        .await;
        // Release a started FIFO open, including when the attach timed out.
        let release = std::fs::OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(fifo);
        let release_error = release.err();
        actor.shutdown().await;
        assert!(ready.is_ok(), "the guest did not reach the FIFO open");
        assert!(blocked, "guest file operation did not block");
        assert!(timed_out, "the guest invocation did not time out");
        assert!(
            release_error.is_none(),
            "failed to release the FIFO reader: {release_error:?}"
        );
        let other = attached
            .expect("mount-free attach was blocked by guest file I/O")
            .unwrap();
        other
            .attachment
            .unwrap()
            .node
            .workload_dispatch
            .shutdown()
            .await;
    });
}

#[cfg(all(feature = "wasm-engine", actr_wasm_fixture_available))]
#[tokio::test]
async fn wasi_data_attach_reads_and_writes_original_directory_after_trap() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("data");
    std::fs::create_dir(&path).unwrap();
    let node = attach_wasi_fixture(
        &root,
        Some((path.clone(), ReadWrite)),
        WasiDataDir::Inherit,
        Some(ReadWrite),
    )
    .await
    .unwrap();
    let actor = node.attachment.unwrap().node.workload_dispatch;
    let writes = call_wasi_fixture(&actor, "test/wasi-data-write", b"before")
        .await
        .unwrap();
    assert_eq!(writes.as_ref(), &1u32.to_le_bytes());
    assert_eq!(
        call_wasi_fixture(&actor, "test/wasi-data-read", b"")
            .await
            .unwrap(),
        &b"before"[..],
    );
    assert_eq!(
        std::fs::read(path.join("fixture-state")).unwrap(),
        b"before"
    );

    #[cfg(unix)]
    let original = {
        let original = root.path().join("original");
        std::fs::rename(&path, &original).unwrap();
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("fixture-state"), b"replacement").unwrap();
        original
    };
    #[cfg(not(unix))]
    let original = path.clone();

    let error = call_wasi_fixture(&actor, "test/wasi-data-trap", b"")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("trapped"), "{error}");
    assert_eq!(
        call_wasi_fixture(&actor, "test/wasi-data-read", b"")
            .await
            .unwrap(),
        &b"before"[..],
    );
    let writes = call_wasi_fixture(&actor, "test/wasi-data-write", b"after")
        .await
        .unwrap();
    assert_eq!(
        writes.as_ref(),
        &1u32.to_le_bytes(),
        "the trap must reset guest memory"
    );
    assert_eq!(
        std::fs::read(original.join("fixture-state")).unwrap(),
        b"after"
    );
    #[cfg(unix)]
    assert_eq!(
        std::fs::read(path.join("fixture-state")).unwrap(),
        b"replacement"
    );
    actor.shutdown().await;
}

#[cfg(all(feature = "wasm-engine", actr_wasm_fixture_available))]
#[tokio::test]
async fn wasi_data_attach_respects_absent_and_disabled_grants() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("data");
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("fixture-state"), b"host only").unwrap();
    for (runtime_path, grant) in [
        (None, WasiDataDir::Inherit),
        (Some((path.clone(), ReadWrite)), WasiDataDir::Disabled),
    ] {
        let result = attach_wasi_fixture(&dir, runtime_path, grant, Some(ReadWrite)).await;
        assert!(matches!(
            result,
            Err(HyperError::Config(ref message))
                if message.contains(package_runtime::WASI_DATA_READ_WRITE)
        ));
    }
    for runtime_path in [None, Some((path.clone(), ReadWrite))] {
        let node = attach_wasi_fixture(&dir, runtime_path, WasiDataDir::Inherit, None)
            .await
            .unwrap();
        let actor = node.attachment.unwrap().node.workload_dispatch;
        let error = call_wasi_fixture(&actor, "test/wasi-data-read", b"")
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("WASI data read failed"),
            "{error}"
        );
        let error = call_wasi_fixture(&actor, "test/wasi-data-write", b"guest")
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("WASI data write failed"),
            "{error}"
        );
        assert_eq!(
            std::fs::read(path.join("fixture-state")).unwrap(),
            b"host only"
        );
        actor.shutdown().await;
    }
}

#[cfg(all(feature = "wasm-engine", actr_wasm_fixture_available))]
#[tokio::test]
async fn wasi_data_attach_read_only_denies_writes_even_after_rebuild() {
    let root = TempDir::new().unwrap();
    let directory = root.path().join("data");
    std::fs::create_dir(&directory).unwrap();
    let state = directory.join("fixture-state");

    for (deployment, grant) in [
        (Some((directory.clone(), ReadOnly)), WasiDataDir::Inherit),
        (Some((directory.clone(), ReadWrite)), WasiDataDir::Inherit),
        (None, WasiDataDir::ReadOnlyDirectory(directory.clone())),
    ] {
        std::fs::write(&state, b"host contents").unwrap();
        let node = attach_wasi_fixture(&root, deployment, grant, Some(ReadOnly))
            .await
            .unwrap();
        let actor = node.attachment.unwrap().node.workload_dispatch;
        for rebuild in [false, true] {
            if rebuild {
                let error = call_wasi_fixture(&actor, "test/wasi-data-trap", b"")
                    .await
                    .unwrap_err();
                assert!(error.to_string().contains("trapped"), "{error}");
            }

            let read = call_wasi_fixture(&actor, "test/wasi-data-read", b"")
                .await
                .unwrap();
            assert_eq!(read.as_ref(), b"host contents");
            let error = call_wasi_fixture(&actor, "test/wasi-data-write", b"guest")
                .await
                .unwrap_err();
            assert!(
                error.to_string().contains("WASI data write failed"),
                "{error}"
            );
            assert_eq!(std::fs::read(&state).unwrap(), b"host contents");
        }

        std::fs::remove_file(&state).unwrap();
        let error = call_wasi_fixture(&actor, "test/wasi-data-write", b"guest")
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("WASI data write failed"),
            "{error}"
        );
        assert!(!state.exists());
        actor.shutdown().await;
    }
}

#[cfg(all(feature = "wasm-engine", actr_wasm_fixture_available))]
#[tokio::test]
async fn wasi_data_attach_rejects_required_write_with_a_read_only_grant() {
    let root = TempDir::new().unwrap();
    let directory = root.path().join("data");
    std::fs::create_dir(&directory).unwrap();

    for (deployment, grant) in [
        (Some((directory.clone(), ReadOnly)), WasiDataDir::Inherit),
        (
            Some((directory.clone(), ReadOnly)),
            WasiDataDir::Directory(directory.clone()),
        ),
        (None, WasiDataDir::ReadOnlyDirectory(directory)),
    ] {
        let result = attach_wasi_fixture(&root, deployment, grant, Some(ReadWrite)).await;
        assert!(matches!(
            result,
            Err(HyperError::Config(ref message))
                if message.contains(package_runtime::WASI_DATA_READ_WRITE)
                    && message.contains("only read-only")
        ));
    }
}

fn dev_config(dir: &TempDir) -> HyperConfig {
    let signing_key = SigningKey::generate(&mut OsRng);
    let pubkey = signing_key.verifying_key().to_bytes();
    HyperConfig::new(
        dir.path(),
        Arc::new(crate::verify::StaticTrust::new(pubkey).unwrap()),
    )
}

#[tokio::test]
async fn init_creates_data_dir_and_instance_id() {
    let dir = TempDir::new().unwrap();
    let sub = dir.path().join("subdir/nested");
    let signing_key = SigningKey::generate(&mut OsRng);
    let config = HyperConfig::new(
        &sub,
        Arc::new(crate::verify::StaticTrust::new(signing_key.verifying_key().to_bytes()).unwrap()),
    );

    let hyper = Hyper::new(config).await.unwrap();
    assert!(sub.exists());
    assert!(!hyper.instance_id().is_empty());
}

#[tokio::test]
async fn instance_id_is_stable_across_reinit() {
    let dir = TempDir::new().unwrap();
    let config1 = dev_config(&dir);
    let hyper1 = Hyper::new(config1).await.unwrap();
    let id1 = hyper1.instance_id().to_string();

    let config2 = dev_config(&dir);
    let hyper2 = Hyper::new(config2).await.unwrap();
    let id2 = hyper2.instance_id().to_string();

    assert_eq!(id1, id2, "instance_id should remain stable across restarts");
}

#[tokio::test]
async fn verify_package_rejects_non_wasm() {
    let dir = TempDir::new().unwrap();
    let hyper = Hyper::new(dev_config(&dir)).await.unwrap();
    let result = hyper
        .verify_package(&WorkloadPackage::new(b"not a wasm file".to_vec()))
        .await;
    assert!(matches!(result, Err(HyperError::InvalidManifest(_))));
}

#[tokio::test]
async fn verify_package_rejects_non_actr_format() {
    let dir = TempDir::new().unwrap();
    let hyper = Hyper::new(dev_config(&dir)).await.unwrap();

    // Non-.actr bytes should return InvalidManifest
    let result = hyper
        .verify_package(&WorkloadPackage::new(b"\0asm\x01\x00\x00\x00".to_vec()))
        .await;
    assert!(matches!(result, Err(HyperError::InvalidManifest(_))));
}

/// A custom [`crate::verify::TrustProvider`] that returns a fixed
/// [`VerifiedRuntimePackage`] without doing real verification — used to prove Hyper
/// re-validates the identity at the trust boundary regardless of the provider.
#[derive(Debug)]
struct FakeTrust {
    verified: VerifiedRuntimePackage,
}

#[async_trait::async_trait]
impl crate::verify::TrustProvider for FakeTrust {
    async fn verify_package(&self, _bytes: &[u8]) -> HyperResult<VerifiedRuntimePackage> {
        Ok(self.verified.clone())
    }
}

#[tokio::test]
async fn verify_package_rejects_invalid_identity_from_custom_provider() {
    let dir = TempDir::new().unwrap();
    let mut bad = fake_manifest();
    // A signed-but-malicious identity: traversal attempt in the version field.
    bad.manifest.version = "../../victim".to_string();
    let config = HyperConfig::new(
        dir.path(),
        Arc::new(FakeTrust {
            verified: bad.into(),
        }),
    );
    let hyper = Hyper::new(config).await.unwrap();
    let result = hyper
        .verify_package(&WorkloadPackage::new(b"anything".to_vec()))
        .await;
    assert!(
        matches!(result, Err(HyperError::InvalidManifest(_))),
        "expected InvalidManifest for unsafe identity, got: {:?}",
        result.err()
    );
}

// ─── Issuer integration tests (mockito mock server) ────────────────────────

/// Helper: build a [`VerifiedPackage`] for tests.
///
/// Uses the canonical `actr_pack::PackageManifest` shape wrapped with empty
/// manifest_raw / sig_raw placeholders — bootstrap tests don't touch Issuer's
/// re-verification path, so those bytes are not inspected.
fn fake_manifest() -> VerifiedPackage {
    VerifiedPackage {
        manifest: actr_pack::PackageManifest {
            publisher: "test-publisher".to_string(),
            name: "TestActor".to_string(),
            version: "0.1.0".to_string(),
            binary: actr_pack::BinaryEntry {
                path: "bin/actor.wasm".to_string(),
                target: "wasm32-wasip1".to_string(),
                hash: "0".repeat(64),
                size: None,
                kind: None,
            },
            signature_algorithm: "ed25519".to_string(),
            signing_key_id: None,
            resources: vec![],
            proto_files: vec![],
            lock_file: None,
            metadata: actr_pack::ManifestMetadata::default(),
        },
        manifest_raw: vec![],
        sig_raw: vec![0u8; 64],
        signing_public_key: [0u8; 32],
    }
}

/// Helper: build valid RegisterResponse protobuf bytes with credential data.
fn fake_register_response_bytes() -> Vec<u8> {
    use actr_protocol::{
        AIdCredential, ActrId, ActrType, IdentityClaims, RegisterResponse, TurnCredential,
        register_response,
    };

    let claims = IdentityClaims {
        realm: "test@example.com".to_string(),
        actor_id: "test-actor-id".to_string(),
        expires_at: u64::MAX,
        protocol_version: 1,
        issuer: "test@example.com".to_string(),
        provider: "provider.example".to_string(),
        audience: "actr.signaling".to_string(),
        actor_type: ActrType::default(),
        member: "member-1".to_string(),
        client_key: "0000000000000000".to_string(),
        role: actr_protocol::MemberRole::Owner as i32,
        development: None,
    };
    let claims_bytes = claims.encode_to_vec();

    let credential = AIdCredential {
        key_id: 1,
        claims: claims_bytes.into(),
        signature: vec![0u8; 64].into(),
    };

    let actr_id = ActrId {
        realm: "test@example.com".to_string(),
        serial_number: 42,
    };

    let turn = TurnCredential {
        protocol_version: 1,
        realm: "test@example.com".to_string(),
        provider: "provider.example".to_string(),
        audience: "actr.turn".to_string(),
        username: "user".to_string(),
        password: "pass".to_string(),
        expires_at: u64::MAX,
    };

    let ok = register_response::RegisterOk {
        actr_id,
        credential,
        turn_credential: turn,
        credential_expires_at: None,
        signaling_heartbeat_interval_secs: 30,
        signing_pubkey: vec![0u8; 32].into(),
        signing_key_id: 1,
        renewal_token: None,
        renewal_token_expires_at: None,
    };

    RegisterResponse {
        result: Some(register_response::Result::Success(ok)),
    }
    .encode_to_vec()
}

fn test_service_spec() -> Option<ServiceSpec> {
    Some(ServiceSpec {
        name: "EchoService".to_string(),
        description: Some("test service".to_string()),
        fingerprint: "fp-123".to_string(),
        protobufs: vec![],
        published_at: None,
        tags: vec!["latest".to_string()],
    })
}

fn test_acl() -> Option<Acl> {
    Some(Acl { rules: vec![] })
}

fn linked_runtime_config(dir: &TempDir) -> actr_config::RuntimeConfig {
    actr_config::RuntimeConfig {
        package: actr_config::PackageInfo {
            name: "LinkedActor".to_string(),
            actr_type: actr_protocol::ActrType {
                publisher: "test-publisher".to_string(),
                name: "LinkedActor".to_string(),
                version: "0.1.0".to_string(),
            },
            description: None,
            authors: vec![],
            license: None,
        },
        signaling_url: url::Url::parse("ws://localhost:8081/signaling/ws").unwrap(),
        realm: "test7@example.com".to_string(),
        issuer_endpoint: "http://localhost:8081/issuer".to_string(),
        realm_client_key: None,
        visible_in_discovery: true,
        acl: test_acl(),
        mailbox_path: None,
        scripts: std::collections::HashMap::new(),
        webrtc: actr_config::WebRtcConfig::default(),
        observability: actr_config::ObservabilityConfig {
            filter_level: "info".to_string(),
            tracing_enabled: false,
            tracing_endpoint: "http://localhost:4317".to_string(),
            tracing_service_name: "linked-test".to_string(),
            logs_enabled: false,
            ca_cert_path: None,
        },
        config_dir: dir.path().to_path_buf(),
        trust: vec![],
        package_path: None,
        web: None,
        storage_root: None,
        wasi_data_dir: None,
        wasi_data_access: actr_config::WasiDataAccess::default(),
    }
}

#[test]
fn linked_register_request_uses_linked_auth_mode() {
    let dir = TempDir::new().unwrap();
    let req = build_linked_register_request(&linked_runtime_config(&dir), test_service_spec());

    assert_eq!(req.auth_mode, Some(RegisterAuthMode::Linked as i32));
    assert_eq!(req.manifest_raw, None);
    assert_eq!(req.publisher_signature, None);
    assert!(req.ws_address.is_none());
}

struct RegistrationTestWorkload;

#[async_trait::async_trait]
impl crate::workload::LinkedWorkloadHandle for RegistrationTestWorkload {}

async fn registration_test_node(dir: &TempDir, package: bool) -> Node<Attached> {
    let hyper = Hyper::new(dev_config(dir)).await.unwrap();
    let mut node = Node::from_hyper(hyper, linked_runtime_config(dir))
        .link_handle(Arc::new(RegistrationTestWorkload))
        .await
        .unwrap();
    if package {
        node.attachment.as_mut().unwrap().verified = Some(fake_manifest().into());
    }
    node
}

#[tokio::test]
async fn registration_preserves_the_sent_request_for_credential_recovery() {
    use crate::lifecycle::credential_manager::RegistrationContext;

    for package in [false, true] {
        let mut server = mockito::Server::new_async().await;
        let received = Arc::new(std::sync::Mutex::new(None));
        let captured = received.clone();
        let mock = server
            .mock("POST", "/register")
            .match_request(move |request| {
                let request = RegisterRequest::decode(request.body().unwrap().as_slice()).unwrap();
                *captured.lock().unwrap() = Some(request);
                true
            })
            .with_status(200)
            .with_header("content-type", "application/x-protobuf")
            .with_body(fake_register_response_bytes())
            .expect(1)
            .create_async()
            .await;
        let dir = TempDir::new().unwrap();
        let registered = registration_test_node(&dir, package)
            .await
            .register_with(&server.url(), test_service_spec())
            .await
            .unwrap();
        mock.assert_async().await;

        let node = registered.attachment.unwrap().node;
        assert!(node.preregistered_credential.is_some());
        let context = node.preregistered_registration_context.unwrap();
        let saved = match (package, context) {
            (true, RegistrationContext::Package { request })
            | (false, RegistrationContext::Linked { request }) => request,
            _ => panic!("credential recovery changed the workload's registration mode"),
        };
        assert_eq!(saved, received.lock().unwrap().take().unwrap());
        assert_eq!(saved.realm, "test7@example.com");
        assert_eq!(saved.service_spec, test_service_spec());
        assert_eq!(saved.acl, test_acl());
        if package {
            assert_eq!(saved.publisher_signature, Some(vec![0; 64].into()));
            assert_eq!(saved.signing_public_key, Some(vec![0; 32].into()));
        } else {
            assert!(saved.ws_address.is_none());
        }
    }
}

#[cfg(feature = "test-utils")]
#[tokio::test]
async fn preregistered_credentials_keep_the_workload_recovery_context() {
    use crate::lifecycle::credential_manager::RegistrationContext;
    use actr_protocol::{RegisterResponse, register_response};

    for package in [false, true] {
        let dir = TempDir::new().unwrap();
        let response = RegisterResponse::decode(fake_register_response_bytes().as_slice()).unwrap();
        let Some(register_response::Result::Success(ok)) = response.result else {
            panic!("fixture must contain a successful registration");
        };
        let registered = registration_test_node(&dir, package)
            .await
            .try_with_preregistered_credential(ok.clone())
            .unwrap();
        let node = registered.attachment.unwrap().node;
        assert_eq!(node.preregistered_credential, Some(ok));
        match (package, node.preregistered_registration_context.unwrap()) {
            (true, RegistrationContext::Package { request }) => {
                assert_eq!(request.realm, "test7@example.com");
                assert_eq!(request.auth_mode, Some(RegisterAuthMode::Package as i32));
                assert_eq!(request.publisher_signature, Some(vec![0; 64].into()));
                assert_eq!(request.signing_public_key, Some(vec![0; 32].into()));
            }
            (false, RegistrationContext::Linked { request }) => {
                assert_eq!(request.auth_mode, Some(RegisterAuthMode::Linked as i32));
                assert_eq!(request.acl, test_acl());
            }
            _ => panic!("pre-registration must preserve the workload's auth mode"),
        }
    }
}

/// Exercise the actual Host request/context builder with public material from
/// the same v2 packer/verifier and fixture consumed by the real Issuer tests.
#[tokio::test]
async fn v2_registration_preserves_original_material_for_credential_recovery() {
    use crate::lifecycle::credential_manager::RegistrationContext;
    use actr_pack::{ActorPackOptions, ActorPackageManifest};
    use std::collections::BTreeMap;

    let key = SigningKey::generate(&mut OsRng);
    let source = include_str!("../../actr-pack/tests/fixtures/registration-v2.toml");
    let package = actr_pack::pack_actor_package(&ActorPackOptions {
        manifest: ActorPackageManifest::from_toml(source).unwrap(),
        payloads: BTreeMap::from([("workload/echo.wasm".into(), b"\0asm\x0d\0\x01\0".to_vec())]),
        signing_key: key.clone(),
    })
    .unwrap();
    let verified = actr_pack::verify_runtime_package_bounded(
        &package,
        &key.verifying_key(),
        actr_pack::DEFAULT_MAX_VERIFIED_ENTRY_BYTES,
    )
    .unwrap();
    let mut server = mockito::Server::new_async().await;
    let received = Arc::new(std::sync::Mutex::new(None));
    let captured = received.clone();
    let mock = server
        .mock("POST", "/register")
        .match_request(move |request| {
            let request = RegisterRequest::decode(request.body().unwrap().as_slice()).unwrap();
            *captured.lock().unwrap() = Some(request);
            true
        })
        .with_status(200)
        .with_header("content-type", "application/x-protobuf")
        .with_body(fake_register_response_bytes())
        .expect(1)
        .create_async()
        .await;
    let dir = TempDir::new().unwrap();
    let mut attached = registration_test_node(&dir, false).await;
    attached.attachment.as_mut().unwrap().verified = Some(verified.clone());
    attached.attachment.as_mut().unwrap().package_bytes = package.into();
    let registered = attached.register(&server.url()).await.unwrap();
    mock.assert_async().await;
    let node = registered.attachment.unwrap().node;
    let Some(RegistrationContext::Package { request: saved }) =
        node.preregistered_registration_context
    else {
        panic!("v2 package must retain package-mode recovery context");
    };
    assert_eq!(saved, received.lock().unwrap().take().unwrap());
    assert_eq!(saved.actr_type, verified.manifest.actor_type());
    assert_eq!(
        saved.manifest_raw.as_deref(),
        Some(verified.manifest_raw.as_slice())
    );
    assert_eq!(
        saved.publisher_signature.as_deref(),
        Some(verified.sig_raw.as_slice())
    );
    assert_eq!(
        saved.signing_public_key.as_deref(),
        Some(key.verifying_key().as_bytes().as_slice())
    );
    assert_eq!(saved.target, None);
    assert_eq!(saved.auth_mode, Some(RegisterAuthMode::Package as i32));
}

#[tokio::test]
async fn with_actor_type_overrides_pending_runtime_metadata() {
    let dir = TempDir::new().unwrap();
    let hyper = Hyper::new(dev_config(&dir)).await.unwrap();
    let node = Node::from_hyper(hyper, linked_runtime_config(&dir)).with_actor_type(
        actr_protocol::ActrType {
            publisher: "acme".into(),
            name: "UnifiedActor".into(),
            version: "1.0.0".into(),
        },
    );

    let actr_type = node.runtime_config().actr_type();
    assert_eq!(actr_type.publisher, "acme");
    assert_eq!(actr_type.name, "UnifiedActor");
    assert_eq!(actr_type.version, "1.0.0");
}

#[tokio::test]
async fn linked_manifest_identity_rejects_actor_type_override() {
    let dir = TempDir::new().unwrap();
    let hyper = Hyper::new(dev_config(&dir)).await.unwrap();
    let manifest_type = actr_protocol::ActrType {
        publisher: "acme".into(),
        name: "ManifestActor".into(),
        version: "1.0.0".into(),
    };
    let manifest = actr_config::ManifestConfig {
        package: actr_config::PackageInfo {
            name: manifest_type.name.clone(),
            actr_type: manifest_type.clone(),
            description: None,
            authors: vec![],
            license: None,
        },
        exports: vec![],
        dependencies: vec![],
        acl: None,
        tags: vec![],
        scripts: std::collections::HashMap::new(),
        binary: None,
        build: None,
        config_dir: dir.path().to_path_buf(),
    };
    let requested_type = actr_protocol::ActrType {
        publisher: "other".into(),
        name: "Override".into(),
        version: "9.9.9".into(),
    };

    let node = Node::from_hyper(hyper, linked_runtime_config(&dir))
        .with_linked_manifest(manifest)
        .with_actor_type(requested_type);

    assert_eq!(node.runtime_config().actr_type(), &manifest_type);
}

#[test]
fn compatible_native_target_matches_current_host() {
    // Current host should always match itself.
    let current = format!(
        "{}-unknown-{}",
        std::env::consts::ARCH,
        if std::env::consts::OS == "macos" {
            "darwin"
        } else {
            std::env::consts::OS
        }
    );
    assert!(
        is_compatible_native_target(&current),
        "current host target `{current}` should be compatible"
    );
}

#[test]
fn compatible_native_target_rejects_cross_platform() {
    // A target for a different arch/os should be rejected.
    assert!(!is_compatible_native_target("riscv64gc-unknown-linux-gnu"));
    assert!(!is_compatible_native_target("s390x-unknown-linux-gnu"));
}

#[test]
fn compatible_native_target_rejects_short_triples() {
    assert!(!is_compatible_native_target("invalid-target"));
    assert!(!is_compatible_native_target("single"));
    assert!(!is_compatible_native_target(""));
}

#[cfg(feature = "dynclib-engine")]
fn fake_dynclib_manifest() -> PackageManifest {
    let target = format!(
        "{}-unknown-{}",
        std::env::consts::ARCH,
        if std::env::consts::OS == "macos" {
            "darwin"
        } else {
            std::env::consts::OS
        }
    );
    PackageManifest {
        publisher: "test-publisher".to_string(),
        name: "DynActor".to_string(),
        version: "1.0.0".to_string(),
        binary: actr_pack::BinaryEntry {
            path: format!("bin/actor{}", dynclib_tempfile_suffix()),
            target,
            hash: String::new(),
            size: None,
            kind: None,
        },
        signature_algorithm: "ed25519".to_string(),
        signing_key_id: None,
        resources: vec![],
        proto_files: vec![],
        lock_file: None,
        metadata: actr_pack::ManifestMetadata::default(),
    }
}

#[cfg(feature = "dynclib-engine")]
fn fake_dynclib_package_bytes(binary_bytes: &[u8]) -> (Vec<u8>, PackageManifest) {
    let manifest = fake_dynclib_manifest();
    let signing_key = SigningKey::generate(&mut OsRng);
    let package_bytes = actr_pack::pack(&actr_pack::PackOptions {
        manifest: manifest.clone(),
        binary_bytes: binary_bytes.to_vec(),
        resources: vec![],
        proto_files: vec![],
        lock_file: None,
        signing_key,
    })
    .unwrap();
    // `pack()` updates the embedded manifest's binary hash; re-parse so
    // the returned manifest agrees with what's actually in the archive.
    let packed_manifest = actr_pack::read_manifest(&package_bytes).unwrap();
    (package_bytes, packed_manifest)
}

#[cfg(feature = "dynclib-engine")]
#[test]
fn dynclib_cache_path_uses_hash_and_platform_suffix() {
    let dir = TempDir::new().unwrap();
    let path = dynclib_cache_path(dir.path(), &[0xAB; 32]);

    assert_eq!(path.parent().unwrap(), dynclib_cache_dir(dir.path()));
    assert_eq!(
        path.file_name().unwrap().to_string_lossy(),
        format!("{}{}", hex::encode([0xAB; 32]), dynclib_tempfile_suffix())
    );
}

#[cfg(feature = "dynclib-engine")]
#[test]
fn legacy_rebuild_does_not_remove_v2_cache_for_the_same_binary() {
    let dir = TempDir::new().unwrap();
    let binary = b"shared dylib bytes";
    let (package, manifest) = fake_dynclib_package_bytes(binary);
    let hash = manifest.binary.hash_bytes().unwrap();
    let legacy_path = dynclib_cache_path(dir.path(), &hash);
    let v2_path = v2_dynclib_cache_path(dir.path(), &hash);

    ensure_v2_dynclib_cache_file(&v2_path, binary).unwrap();
    std::fs::write(&legacy_path, b"corrupt").unwrap();
    rebuild_dynclib_cache_file(&legacy_path, &package, &manifest).unwrap();

    assert_ne!(legacy_path, v2_path);
    assert_eq!(std::fs::read(&legacy_path).unwrap(), binary);
    assert_eq!(std::fs::read(&v2_path).unwrap(), binary);
}

#[cfg(feature = "dynclib-engine")]
#[test]
fn v2_dynclib_cache_replaces_corrupted_bytes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("cached.so");
    std::fs::write(&path, b"invalid data").unwrap();
    ensure_v2_dynclib_cache_file(&path, b"signed bytes").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"signed bytes");
    ensure_v2_dynclib_cache_file(&path, b"signed bytes").unwrap();
}

#[cfg(feature = "dynclib-engine")]
#[test]
fn v2_dynclib_cache_stays_openable_during_concurrent_repairs() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("cached.so");
    let binary = vec![0x5a; 4 * 1024 * 1024];
    std::fs::write(&path, b"invalid data").unwrap();

    let start = Barrier::new(5);
    std::thread::scope(|scope| {
        let repairs: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    start.wait();
                    ensure_v2_dynclib_cache_file(&path, &binary).unwrap();
                    assert!(v2_dynclib_cache_matches(&path, &binary));
                })
            })
            .collect();

        start.wait();
        loop {
            std::fs::File::open(&path).expect("the cache path must remain openable during repair");
            if repairs.iter().all(|repair| repair.is_finished()) {
                break;
            }
            std::thread::yield_now();
        }
    });
}

#[cfg(all(feature = "dynclib-engine", unix))]
#[test]
fn v2_dynclib_cache_does_not_follow_a_symlink() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("target");
    let path = dir.path().join("cached.so");
    std::fs::write(&target, b"signed bytes").unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    ensure_v2_dynclib_cache_file(&path, b"signed bytes").unwrap();
    assert!(!std::fs::symlink_metadata(&path).unwrap().is_symlink());
    assert_eq!(std::fs::read(&target).unwrap(), b"signed bytes");
}

fn fake_v2_native_package(
    key: &SigningKey,
    configure: impl FnOnce(&mut actr_pack::ActorPackageManifest),
) -> WorkloadPackage {
    use actr_pack::{ActorPackOptions, ActorPackageManifest};
    use std::collections::BTreeMap;

    let mut manifest = ActorPackageManifest::from_toml(include_str!(
        "../../actr-pack/tests/fixtures/registration-v2.toml"
    ))
    .unwrap();
    manifest.workload.profile = "actr.native/1".into();
    manifest.workload.entry = "bin/test.so".into();
    manifest.files[0].path = manifest.workload.entry.clone();
    manifest.files[0].media_type = "application/octet-stream".into();
    configure(&mut manifest);
    WorkloadPackage::new(
        actr_pack::pack_actor_package(&ActorPackOptions {
            manifest,
            payloads: BTreeMap::from([("bin/test.so".into(), b"not a library".to_vec())]),
            signing_key: key.clone(),
        })
        .unwrap(),
    )
}

#[tokio::test]
async fn v2_native_package_loading_is_gated_by_dynclib_engine() {
    let dir = TempDir::new().unwrap();
    let key = SigningKey::generate(&mut OsRng);
    let package = fake_v2_native_package(&key, |_| {});
    let trust = StaticTrust::new(key.verifying_key().to_bytes()).unwrap();
    let config = HyperConfig::new(dir.path().join("hyper"), Arc::new(trust));
    let hyper = Hyper::new(config).await.unwrap();
    let error = match Node::from_hyper(hyper, linked_runtime_config(&dir))
        .attach(&package)
        .await
    {
        Ok(_) => panic!("a non-library must not load"),
        Err(error) => error.to_string(),
    };
    let expected = if cfg!(feature = "dynclib-engine") {
        "failed to load native package"
    } else {
        "native package requires the `dynclib-engine` feature"
    };
    assert!(error.contains(expected), "unexpected error: {error}");
}

#[tokio::test]
async fn v2_native_package_rejects_untrusted_signature_before_loading() {
    let dir = TempDir::new().unwrap();
    let key = SigningKey::generate(&mut OsRng);
    let other_key = SigningKey::generate(&mut OsRng);
    let package = fake_v2_native_package(&other_key, |_| {});
    let trust = StaticTrust::new(key.verifying_key().to_bytes()).unwrap();
    let config = HyperConfig::new(dir.path(), Arc::new(trust));
    let hyper = Hyper::new(config).await.unwrap();
    let error = hyper.load_workload_package(&package).await.err().unwrap();
    assert!(
        matches!(error, HyperError::SignatureVerificationFailed(_)),
        "unexpected error: {error}"
    );
    assert!(!dir.path().join("dynclib-cache").exists());
}

#[tokio::test]
async fn v2_native_package_rejects_unsupported_host_requirements_before_loading() {
    for (api, required, optional, expected) in [
        (
            ">=2.0.0",
            None,
            None,
            "native host API 1.0.0 does not satisfy",
        ),
        (
            "^1.0.0",
            Some("wasi.data.read-write"),
            None,
            "actr.native/1 does not support host capabilities",
        ),
        (
            "^1.0.0",
            None,
            Some("wasi.data.read-only"),
            "actr.native/1 does not support host capabilities",
        ),
    ] {
        let dir = TempDir::new().unwrap();
        let key = SigningKey::generate(&mut OsRng);
        let package = fake_v2_native_package(&key, |manifest| {
            manifest.host.api = api.into();
            manifest.host.required_capabilities = required.into_iter().map(String::from).collect();
            manifest.host.optional_capabilities = optional.into_iter().map(String::from).collect();
        });
        let trust = StaticTrust::new(key.verifying_key().to_bytes()).unwrap();
        let config = HyperConfig::new(dir.path(), Arc::new(trust));
        let hyper = Hyper::new(config).await.unwrap();
        let error = hyper.load_workload_package(&package).await.err().unwrap();
        assert!(
            matches!(error, HyperError::InvalidManifest(_)) && error.to_string().contains(expected),
            "unexpected error: {error}"
        );
        assert!(!dir.path().join("dynclib-cache").exists());
    }
}

#[cfg(feature = "dynclib-engine")]
#[test]
fn ensure_dynclib_cache_path_preserves_existing_file() {
    let dir = TempDir::new().unwrap();
    let initial_binary_bytes = b"initial dylib bytes";
    let (initial_package_bytes, manifest) = fake_dynclib_package_bytes(initial_binary_bytes);
    let cache_path =
        ensure_dynclib_cache_path(dir.path(), &initial_package_bytes, &manifest).unwrap();

    // Same initial binary -> same manifest.binary.hash -> same cache path;
    // a second call with a different binary under that hash cannot land
    // here, so re-run with the identical binary to assert idempotence.
    let second_path =
        ensure_dynclib_cache_path(dir.path(), &initial_package_bytes, &manifest).unwrap();

    assert_eq!(cache_path, second_path);
    assert_eq!(std::fs::read(&cache_path).unwrap(), initial_binary_bytes);
}

#[cfg(feature = "dynclib-engine")]
#[test]
fn ensure_dynclib_cache_path_handles_concurrent_creation() {
    let dir = TempDir::new().unwrap();
    let binary_bytes = b"shared dylib bytes".to_vec();
    let (package_bytes, manifest) = fake_dynclib_package_bytes(&binary_bytes);
    let package_bytes = Arc::new(package_bytes);
    let binary_bytes = Arc::new(binary_bytes);
    let data_dir = Arc::new(dir.path().to_path_buf());
    let barrier = Arc::new(Barrier::new(3));

    let handles: Vec<_> = (0..2)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            let data_dir = Arc::clone(&data_dir);
            let manifest = manifest.clone();
            let package_bytes = Arc::clone(&package_bytes);
            std::thread::spawn(move || {
                barrier.wait();
                ensure_dynclib_cache_path(&data_dir, &package_bytes, &manifest)
            })
        })
        .collect();

    barrier.wait();

    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap().unwrap())
        .collect();

    assert_eq!(results[0], results[1]);
    assert_eq!(
        std::fs::read(&results[0]).unwrap(),
        binary_bytes.as_ref().as_slice()
    );
}

/// Package bootstrap always authenticates with the publisher manifest and does
/// not read or write legacy PSK keys.
#[tokio::test]
async fn bootstrap_package_uses_manifest_auth() {
    let response_body = fake_register_response_bytes();

    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/register")
        .with_status(200)
        .with_header("content-type", "application/x-protobuf")
        .with_body(response_body)
        .expect(1)
        .create_async()
        .await;

    let dir = TempDir::new().unwrap();
    let config = dev_config(&dir);
    let hyper = Hyper::new(config).await.unwrap();

    let manifest = fake_manifest();
    let result = hyper
        .bootstrap_credential(
            &manifest.clone().into(),
            &server.url(),
            "test@example.com",
            test_service_spec(),
            test_acl(),
            None,
        )
        .await;

    mock.assert_async().await;
    assert!(
        result.is_ok(),
        "Package registration should succeed, got: {:?}",
        result.err()
    );

    // Verify no legacy PSK keys are written to ActorStore.
    let namespace =
        storage_namespace_for(&RuntimePackageManifest::Legacy(manifest.manifest)).unwrap();
    let root = SecureStorageRoot::new(hyper.config().storage_root())
        .await
        .unwrap();
    let store = ActorStore::open_under(&root, &namespace).await.unwrap();
    assert!(
        store.kv_get("hyper:psk:token").await.unwrap().is_none(),
        "PSK token must not be stored after PSK removal"
    );
}

/// Issuer errors should propagate as HyperError::IssuerBootstrapFailed.
#[tokio::test]
async fn bootstrap_issuer_error_propagates() {
    use actr_protocol::{ErrorResponse, RegisterResponse, register_response};

    let error_resp = RegisterResponse {
        result: Some(register_response::Result::Error(ErrorResponse {
            code: 403,
            message: "publisher not trusted".to_string(),
        })),
    }
    .encode_to_vec();

    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("POST", "/register")
        .with_status(200)
        .with_header("content-type", "application/x-protobuf")
        .with_body(error_resp)
        .create_async()
        .await;

    let dir = TempDir::new().unwrap();
    let config = dev_config(&dir);
    let hyper = Hyper::new(config).await.unwrap();

    let manifest = fake_manifest();
    let result = hyper
        .bootstrap_credential(
            &manifest.clone().into(),
            &server.url(),
            "test@example.com",
            test_service_spec(),
            test_acl(),
            None,
        )
        .await;

    assert!(
        matches!(result, Err(HyperError::IssuerBootstrapFailed(_))),
        "Issuer errors should propagate as IssuerBootstrapFailed, got: {:?}",
        result
    );
}

/// A rejected Issuer registration must not create the per-actor database
/// (issue #350 acceptance; repeated rejected identities must not accumulate
/// empty databases). The store is opened only after a successful response.
#[tokio::test]
async fn bootstrap_issuer_rejection_creates_no_database() {
    use actr_protocol::{ErrorResponse, RegisterResponse, register_response};

    let error_resp = RegisterResponse {
        result: Some(register_response::Result::Error(ErrorResponse {
            code: 403,
            message: "publisher not trusted".to_string(),
        })),
    }
    .encode_to_vec();

    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("POST", "/register")
        .with_status(200)
        .with_header("content-type", "application/x-protobuf")
        .with_body(error_resp)
        .create_async()
        .await;

    let dir = TempDir::new().unwrap();
    let config = dev_config(&dir);
    let hyper = Hyper::new(config).await.unwrap();

    let manifest = fake_manifest();
    let result = hyper
        .bootstrap_credential(
            &manifest.clone().into(),
            &server.url(),
            "test@example.com",
            test_service_spec(),
            test_acl(),
            None,
        )
        .await;
    assert!(matches!(result, Err(HyperError::IssuerBootstrapFailed(_))));

    // The would-be database path must not exist after a rejected registration.
    let namespace =
        storage_namespace_for(&RuntimePackageManifest::Legacy(manifest.manifest)).unwrap();
    let root = SecureStorageRoot::new(hyper.config().storage_root())
        .await
        .unwrap();
    let db_path = root.path_for(&namespace).unwrap();
    assert!(
        !db_path.exists(),
        "rejected registration must not create a database at {}",
        db_path.display()
    );
}

// ── pure helper functions ───────────────────────────────────────────────

use actr_protocol::{ActrType, RegisterAuthMode};

#[test]
fn linked_registration_does_not_advertise_a_peer_websocket_endpoint() {
    let dir = TempDir::new().unwrap();
    let cfg = linked_runtime_config(&dir);
    let req = build_linked_register_request(&cfg, None);
    assert_eq!(req.actr_type, cfg.actr_type().clone());
    assert_eq!(req.realm, cfg.realm);
    assert_eq!(req.auth_mode, Some(RegisterAuthMode::Linked as i32));
    assert!(req.ws_address.is_none());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn is_compatible_native_target_validates_host_triple() {
    // The current host triple must be accepted.
    let host = format!(
        "{}-unknown-{}",
        std::env::consts::ARCH,
        if std::env::consts::OS == "macos" {
            "darwin"
        } else {
            std::env::consts::OS
        }
    );
    assert!(is_compatible_native_target(&host));

    // Bogus / too-short triples are rejected.
    assert!(!is_compatible_native_target("not-a-triple"));
    assert!(!is_compatible_native_target("only-two"));
    assert!(!is_compatible_native_target(""));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn detect_binary_kind_rejects_unsupported_target() {
    let manifest = actr_pack::PackageManifest {
        publisher: "m".into(),
        name: "n".into(),
        version: "1.0.0".into(),
        binary: actr_pack::BinaryEntry {
            path: "bin/x".into(),
            target: "bogus-arch-unknown-os".into(),
            hash: String::new(),
            size: None,
            kind: None,
        },
        signature_algorithm: "ed25519".into(),
        signing_key_id: None,
        resources: vec![],
        proto_files: vec![],
        lock_file: None,
        metadata: actr_pack::ManifestMetadata::default(),
    };
    let err = detect_binary_kind(&manifest).unwrap_err();
    assert!(matches!(err, HyperError::InvalidManifest(_)));
}

#[test]
fn linked_dependency_metadata_loads_from_explicit_manifest_and_ignores_lock() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("manifest.toml"),
        r#"[package]
name = "LinkedClient"
publisher = "acme"
version = "1.0.0"

[dependencies]
echo = { actr_type = "acme/EchoActor@1.0.0", service = "EchoService:service_semantic:echo" }
"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("manifest.lock.toml"),
        "[metadata]\nversion = 1\ngenerated_at = \"2026-08-06T00:00:00Z\"\n",
    )
    .unwrap();

    let manifest =
        actr_config::ConfigParser::from_manifest_file(dir.path().join("manifest.toml")).unwrap();
    let metadata = resolve_linked_dependency_metadata(&manifest).unwrap();
    assert_eq!(
        metadata
            .aliases
            .get("echo")
            .map(ActrType::to_string_repr)
            .as_deref(),
        Some("acme/EchoActor@1.0.0")
    );
    assert_eq!(
        metadata.fingerprints.values().next().map(String::as_str),
        Some("service_semantic:echo")
    );

    let mut duplicate_alias_manifest = manifest.clone();
    let mut duplicate = duplicate_alias_manifest.dependencies[0].clone();
    duplicate.actr_type = Some(ActrType {
        publisher: "acme".into(),
        name: "OtherEchoActor".into(),
        version: "1.0.0".into(),
    });
    duplicate_alias_manifest.dependencies.push(duplicate);
    let error = resolve_linked_dependency_metadata(&duplicate_alias_manifest)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("alias `echo` is declared more than once"),
        "{error}"
    );

    std::fs::write(
        dir.path().join("manifest.toml"),
        r#"[package]
name = "LinkedClient"
publisher = "acme"
version = "1.0.0"

[dependencies]
echo = {}
"#,
    )
    .unwrap();
    let manifest =
        actr_config::ConfigParser::from_manifest_file(dir.path().join("manifest.toml")).unwrap();
    let error = resolve_linked_dependency_metadata(&manifest)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("must declare a complete actr_type"),
        "{error}"
    );

    std::fs::write(
        dir.path().join("manifest.toml"),
        r#"[package]
name = "LinkedClient"
publisher = "acme"
version = "1.0.0"

[dependencies]
echo = { actr_type = "acme/EchoActor@1.0.0" }
echo_copy = { actr_type = "acme/EchoActor@1.0.0" }
"#,
    )
    .unwrap();
    let manifest =
        actr_config::ConfigParser::from_manifest_file(dir.path().join("manifest.toml")).unwrap();
    let error = resolve_linked_dependency_metadata(&manifest)
        .unwrap_err()
        .to_string();
    assert!(error.contains("more than one dependency alias"), "{error}");
}
