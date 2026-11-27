#![cfg(all(
    feature = "oci-engine",
    feature = "test-utils",
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]

use std::fs::{self, File};
use std::path::PathBuf;
use std::sync::Arc;

use actr_config::{ConfigParser, OciRuntimeConfig, PackageInfo};
use actr_hyper::test_support::TestSignalingServer;
use actr_hyper::{Hyper, HyperConfig, Node, StaticTrust};
use actr_pack::oci::{OciLimits, pack_oci_package};
use actr_pack::{ActorPackageManifest, RuntimePackageManifest};
use actr_protocol::RpcRequest;
use ed25519_dalek::SigningKey;
use prost::Message;
use sha2::{Digest, Sha256};

#[derive(Clone, PartialEq, Message)]
struct Ping {
    #[prost(bytes, tag = "1")]
    data: Vec<u8>,
}

impl RpcRequest for Ping {
    type Response = Self;

    fn route_key() -> &'static str {
        "echo.Echo.Ping"
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the provisioned Linux OCI test environment"]
async fn starts_from_a_verified_complete_image_and_reclaims_the_container() -> anyhow::Result<()> {
    let fixture =
        PathBuf::from(std::env::var_os("ACTR_OCI_TEST_LAYOUT").expect("ACTR_OCI_TEST_LAYOUT"));
    let config =
        PathBuf::from(std::env::var_os("ACTR_OCI_TEST_SETTINGS").expect("ACTR_OCI_TEST_SETTINGS"));
    let mut host: OciRuntimeConfig = toml::from_str(&fs::read_to_string(config)?)?;
    let key = SigningKey::from_bytes(&[7; 32]);
    let dir = tempfile::tempdir()?;
    let archive = dir.path().join("echo.actr");
    let manifest =
        ActorPackageManifest::from_toml(&fs::read_to_string(fixture.join("manifest.toml"))?)?;
    pack_oci_package(
        manifest,
        &fixture,
        &key,
        File::create(&archive)?,
        OciLimits::default(),
    )?;
    // The smoke fixture is small. Test the production streaming verifier
    // against an independently calculated whole-archive digest.
    host.package_sha256 = hex::encode(Sha256::digest(fs::read(&archive)?));
    host.supervisor = PathBuf::from(env!("CARGO_BIN_EXE_actr-oci-supervisor"));
    host.allowed_capabilities = vec!["actor.receive".into()];

    verify_unpack_reconciliation(&host).await?;

    let mut signaling = TestSignalingServer::start().await?;
    let url = signaling.url();
    let issuer = format!("http://127.0.0.1:{}/issuer", signaling.port());
    let text = format!(
        "[signaling]\nurl={url:?}\n[issuer_endpoint]\nurl={issuer:?}\n[deployment]\nrealm='test7@example.com'\n"
    );
    let path = dir.path().join("actr.toml");
    fs::write(&path, &text)?;
    let repetitions: usize = std::env::var("ACTR_OCI_TEST_RESTARTS")
        .ok()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(1);
    let calls: usize = std::env::var("ACTR_OCI_TEST_CALLS")
        .ok()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(3);

    use std::os::unix::fs::{PermissionsExt, symlink};

    let supervisor = dir.path().join("pause-supervisor");
    let waiting = dir.path().join("supervisor-waiting");
    let resume = dir.path().join("supervisor-resume");
    symlink(&host.supervisor, dir.path().join("real-supervisor"))?;
    fs::write(
        &supervisor,
        r#"#!/bin/sh
set -eu
dir=${0%/*}
touch "$dir/supervisor-waiting"
for attempt in $(seq 1 500); do
    if test -f "$dir/supervisor-resume"; then
        exec "$dir/real-supervisor" "$@"
    fi
    sleep 0.02
done
exit 1
"#,
    )?;
    fs::set_permissions(&supervisor, fs::Permissions::from_mode(0o700))?;

    for run in 0..repetitions {
        let mut deployment = host.clone();
        if run == 0 {
            deployment.supervisor = supervisor.clone();
        }
        let hyper = Hyper::new(HyperConfig::new(
            dir.path().join("hyper"),
            Arc::new(StaticTrust::new(key.verifying_key().to_bytes())?),
        ))
        .await?
        .with_oci_runtime(deployment)?;
        let verified = hyper
            .verify_oci_package(&archive, OciLimits::default())
            .await?;
        assert!(matches!(verified.manifest(), RuntimePackageManifest::V2(_)));
        assert_eq!(hex::encode(verified.archive_digest()), host.package_sha256);
        let runtime = ConfigParser::from_runtime_file(
            &path,
            PackageInfo {
                name: "OciEcho".into(),
                actr_type: verified.manifest().actor_type(),
                description: None,
                authors: Vec::new(),
                license: None,
            },
        )?;
        let node = Node::from_hyper(hyper, runtime)
            .attach_verified_oci(verified)
            .await?
            .register(&issuer)
            .await?;
        let actor = if run == 0 {
            let collect_during_startup = async {
                wait_for_marker(&waiting).await?;
                let collected = actr_hyper::oci::gc(&host).await;
                fs::write(&resume, [])?;
                assert_eq!(collected?, (0, 0), "GC reclaimed a starting instance");
                Ok::<_, anyhow::Error>(())
            };
            let (actor, ()) = tokio::try_join!(
                async { Ok::<_, anyhow::Error>(node.start().await?) },
                collect_during_startup,
            )?;
            actor
        } else {
            node.start().await?
        };
        let invocations = if run == 0 { calls } else { 1 };
        for call in 0..invocations {
            let data = format!("oci-{run}-{call}").into_bytes();
            let response = actor.call(Ping { data: data.clone() }, 5_000).await?;
            assert_eq!(response.data, data);
        }
        actor.shutdown();
        actor.wait_for_shutdown().await;
        assert!(
            fs::read_dir(&host.state_dir)?.next().is_none(),
            "OCI instance resources were not reclaimed"
        );
    }
    signaling.shutdown().await;
    Ok(())
}

async fn verify_unpack_reconciliation(host: &OciRuntimeConfig) -> anyhow::Result<()> {
    use std::io::Write as _;
    use std::os::unix::ffi::OsStrExt as _;
    use std::time::Duration;

    let name = format!("actr-unpack-{}", uuid::Uuid::new_v4().simple());
    let record = host.state_dir.join(format!("{name}.lock"));
    let group = host.cgroup_parent.join(&name);
    let mut owner = File::create_new(&record)?;
    owner.lock()?;
    owner.write_all(host.cgroup_parent.canonicalize()?.as_os_str().as_bytes())?;
    fs::create_dir(&group)?;
    let mut child = tokio::process::Command::new("sleep")
        .arg("60")
        .kill_on_drop(true)
        .spawn()?;
    fs::write(group.join("cgroup.procs"), child.id().unwrap().to_string())?;

    actr_hyper::oci::gc(host).await?;
    assert!(record.exists() && group.exists());
    assert!(child.try_wait()?.is_none(), "GC stopped an owned unpack");
    drop(owner);

    actr_hyper::oci::gc(host).await?;
    assert!(!record.exists() && !group.exists());
    tokio::time::timeout(Duration::from_secs(5), child.wait()).await??;
    Ok(())
}

#[derive(Clone, PartialEq, Message)]
struct Burst {}

impl RpcRequest for Burst {
    type Response = Burst;

    fn route_key() -> &'static str {
        "echo.Echo.Burst"
    }
}

#[derive(Clone, PartialEq, Message)]
struct Large {}

impl RpcRequest for Large {
    type Response = Large;

    fn route_key() -> &'static str {
        "echo.Echo.Large"
    }
}

#[derive(Clone, PartialEq, Message)]
struct Proxy {
    #[prost(bytes, tag = "1")]
    data: Vec<u8>,
}

impl RpcRequest for Proxy {
    type Response = Self;

    fn route_key() -> &'static str {
        "echo.Echo.Proxy"
    }
}

#[derive(Clone, PartialEq, Message)]
struct Fetch {
    #[prost(bytes, tag = "1")]
    data: Vec<u8>,
}

impl RpcRequest for Fetch {
    type Response = Self;

    fn route_key() -> &'static str {
        "echo.Echo.Fetch"
    }
}

#[derive(Clone, PartialEq, Message)]
struct Block {}

impl RpcRequest for Block {
    type Response = Self;

    fn route_key() -> &'static str {
        "echo.Echo.Block"
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the provisioned Linux OCI test environment"]
async fn enforces_file_https_and_actor_capabilities() -> anyhow::Result<()> {
    use actr_config::lock::{
        LOCK_FILE_VERSION, LockFile, LockMetadata, LockedDependency, LockedProtoFile,
    };
    use actr_pack::{ActorDependency, PackageFileEntry, oci};
    use actr_service_compat::{ProtoFile, build_self_contained_service_spec};
    use anyhow::Context as _;

    let fixture =
        PathBuf::from(std::env::var_os("ACTR_OCI_TEST_LAYOUT").expect("ACTR_OCI_TEST_LAYOUT"));
    let settings =
        PathBuf::from(std::env::var_os("ACTR_OCI_TEST_SETTINGS").expect("ACTR_OCI_TEST_SETTINGS"));
    let mut host: OciRuntimeConfig = toml::from_str(&fs::read_to_string(settings)?)?;
    host.supervisor = PathBuf::from(env!("CARGO_BIN_EXE_actr-oci-supervisor"));
    let data_dir = host
        .data
        .as_ref()
        .expect("a bounded data volume")
        .path
        .clone();
    let key = SigningKey::from_bytes(&[7; 32]);
    let dir = tempfile::tempdir()?;
    let mut signaling = TestSignalingServer::start_with_relay().await?;
    let issuer = format!("http://127.0.0.1:{}/issuer", signaling.port());
    let runtime = dir.path().join("actr.toml");
    fs::write(
        &runtime,
        format!(
            "[signaling]\nurl={:?}\n[issuer_endpoint]\nurl={issuer:?}\n[deployment]\nrealm='test7@example.com'\n",
            signaling.url()
        ),
    )?;

    let original =
        ActorPackageManifest::from_toml(&fs::read_to_string(fixture.join("manifest.toml"))?)?;
    let mut target_manifest = original.clone();
    target_manifest.actor.name = "OciTarget".into();
    let target_type = target_manifest.actor_type();
    let contract = fs::read_to_string(fixture.join("contracts/echo.proto"))?;
    let spec = build_self_contained_service_spec(
        &target_manifest.actor.name,
        ProtoFile {
            name: "echo.proto".into(),
            content: contract.clone(),
            path: None,
        },
    )?;
    let target_archive = dir.path().join("target.actr");
    pack_oci_package(
        target_manifest,
        &fixture,
        &key,
        File::create(&target_archive)?,
        OciLimits::default(),
    )?;
    let target_digest = hex::encode(Sha256::digest(fs::read(&target_archive)?));
    let mut target_host = host.clone();
    target_host.package_sha256 = target_digest.clone();
    target_host.allowed_capabilities = vec![oci::ACTOR_RECEIVE.into()];
    let target = launch(
        &key,
        &runtime,
        &issuer,
        dir.path().join("target-hyper"),
        target_host,
        &target_archive,
    )
    .await?;

    let caller_folder = dir.path().join("caller");
    for entry in &original.files {
        let dest = caller_folder.join(&entry.path);
        fs::create_dir_all(dest.parent().unwrap())?;
        fs::copy(fixture.join(&entry.path), dest)?;
    }
    let mut caller = original;
    caller
        .host
        .required_capabilities
        .extend([oci::DATA_READ_WRITE.into(), "actor.call".into()]);
    caller
        .workload
        .oci
        .as_mut()
        .unwrap()
        .env
        .insert("ACTR_ECHO_DATA".into(), "/data/start-marker".into());
    caller.dependencies.insert(
        "echo".into(),
        ActorDependency {
            actor: "local/OciTarget".into(),
            version: "=1.0.0".into(),
            contract: Some("contracts/upstream.proto".into()),
            required: true,
        },
    );

    let mut payload = |path: &str, role: &str, contents: String| -> anyhow::Result<()> {
        fs::write(caller_folder.join(path), contents)?;
        caller.files.push(PackageFileEntry {
            path: path.into(),
            role: role.into(),
            media_type: "text/plain".into(),
            size: 0,
            sha256: "0".repeat(64),
        });
        Ok(())
    };
    payload("contracts/upstream.proto", "contract", contract)?;
    let lock = LockFile {
        metadata: Some(LockMetadata {
            version: LOCK_FILE_VERSION,
            generated_at: "2026-01-01T00:00:00Z".into(),
        }),
        dependencies: vec![LockedDependency {
            name: "OciTarget".into(),
            alias: "echo".into(),
            actr_type: target_type.clone(),
            package_sha256: Some(target_digest.clone()),
            description: None,
            fingerprint: spec.fingerprint.clone(),
            published_at: None,
            tags: Vec::new(),
            cached_at: "2026-01-01T00:00:00Z".into(),
            files: vec![LockedProtoFile {
                path: "echo.proto".into(),
                fingerprint: spec.protobufs[0].fingerprint.clone(),
            }],
        }],
    };
    payload("manifest.lock.toml", "lock", toml::to_string(&lock)?)?;
    let source_type = caller.actor_type();
    let mut privacy = format!(
        r#"
[privacy_contract]
schema_version = 1
release_id = "{source_type}"

[[privacy_contract.actor_role]]
role = "source"
actr_type = "{source_type}"

[[privacy_contract.actor_role]]
role = "target"
actr_type = "{target_type}"

[[privacy_contract.edge]]
source = "source"
target = "target"
data_classes = ["public"]
capabilities = ["actor.call"]
dependency_alias = "echo"
rpc_routes = ["echo.Echo.Ping"]
"#
    );
    let https = std::env::var("ACTR_OCI_TEST_HTTPS_URL")
        .ok()
        .filter(|url| !url.is_empty());
    if let Some(url) = &https {
        privacy.push_str(&format!(
            r#"
[[privacy_contract.terminal]]
alias = "health"
source = "source"
url = {url:?}
method = "HEAD"
required = true
data_classes = ["public"]
"#
        ));
    }
    // Release the helper's mutable borrow before editing other manifest fields.
    fs::write(caller_folder.join("contracts/privacy.toml"), &privacy)?;
    caller.files.push(PackageFileEntry {
        path: "contracts/privacy.toml".into(),
        role: "contract".into(),
        media_type: "text/plain".into(),
        size: 0,
        sha256: "0".repeat(64),
    });
    if let Some(url) = &https {
        caller.host.required_capabilities.push("net.https".into());
        host.https.push(actr_config::OciHttpsGrant {
            alias: "health".into(),
            url: url.clone(),
            method: "HEAD".into(),
            data_classes: vec!["public".into()],
        });
    }
    host.allowed_capabilities = caller.host.required_capabilities.clone();
    host.actors.push(actr_config::OciActorGrant {
        alias: "echo".into(),
        target: target.actor_id().to_string_repr(),
        actor_type: target_type,
        package_sha256: target_digest,
        contract_fingerprint: spec.fingerprint,
        routes: vec!["echo.Echo.Ping".into()],
        data_classes: vec!["public".into()],
        allow_tell: false,
    });
    let mut cancel_guest = caller.clone();
    cancel_guest
        .workload
        .oci
        .as_mut()
        .unwrap()
        .env
        .insert("ACTR_ECHO_DATA".into(), "/data/cancel-start".into());
    let mut readonly = caller.clone();
    readonly
        .host
        .required_capabilities
        .retain(|capability| capability != oci::DATA_READ_WRITE);
    readonly
        .host
        .required_capabilities
        .push(oci::DATA_READ_ONLY.into());
    let env = &mut readonly.workload.oci.as_mut().unwrap().env;
    env.remove("ACTR_ECHO_DATA");
    env.insert("ACTR_ECHO_READ_ONLY".into(), "/data/start-marker".into());

    let caller_archive = dir.path().join("caller.actr");
    pack_oci_package(
        caller,
        &caller_folder,
        &key,
        File::create(&caller_archive)?,
        OciLimits::default(),
    )?;
    host.package_sha256 = hex::encode(Sha256::digest(fs::read(&caller_archive)?));
    let actor = launch(
        &key,
        &runtime,
        &issuer,
        dir.path().join("caller-hyper"),
        host.clone(),
        &caller_archive,
    )
    .await?;
    let writable_host = host.clone();
    assert_eq!(actr_hyper::oci::gc(&host).await?, (0, 0));
    assert_eq!(fs::read(data_dir.join("start-marker"))?, b"started");
    let route = actor
        .call(
            Proxy {
                data: b"scoped outbound actor".to_vec(),
            },
            15_000,
        )
        .await
        .context("scoped outbound Actor call")?;
    assert_eq!(route.data, b"scoped outbound actor");
    actor
        .call(Burst {}, 15_000)
        .await
        .context("Guest callback burst")?;
    let oversized = actor.call(Large {}, 5_000).await;
    assert!(matches!(
        oversized,
        Err(actr_protocol::ActrError::Internal(message)) if message.contains("IPC limit")
    ));
    assert_eq!(
        actor
            .call(
                Ping {
                    data: b"after oversized".to_vec()
                },
                5_000
            )
            .await?
            .data,
        b"after oversized"
    );
    let result = actor.call(Fetch { data: Vec::new() }, 10_000).await;
    if https.is_some() {
        assert!(result?.data.is_empty());
    } else {
        assert!(matches!(
            result,
            Err(actr_protocol::ActrError::PermissionDenied(_))
        ));
    }

    actor.shutdown();
    actor.wait_for_shutdown().await;
    let readonly_archive = dir.path().join("readonly.actr");
    host.allowed_capabilities = readonly.host.required_capabilities.clone();
    pack_oci_package(
        readonly,
        &caller_folder,
        &key,
        File::create(&readonly_archive)?,
        OciLimits::default(),
    )?;
    host.package_sha256 = hex::encode(Sha256::digest(fs::read(&readonly_archive)?));
    let actor = launch(
        &key,
        &runtime,
        &issuer,
        dir.path().join("readonly-hyper"),
        host.clone(),
        &readonly_archive,
    )
    .await?;
    assert_eq!(
        actor
            .call(
                Ping {
                    data: b"readonly".to_vec()
                },
                5_000
            )
            .await?
            .data,
        b"readonly"
    );
    actor.shutdown();
    actor.wait_for_shutdown().await;
    let cancel_archive = dir.path().join("cancel.actr");
    pack_oci_package(
        cancel_guest,
        &caller_folder,
        &key,
        File::create(&cancel_archive)?,
        OciLimits::default(),
    )?;
    let mut writable_host = writable_host;
    writable_host.package_sha256 = hex::encode(Sha256::digest(fs::read(&cancel_archive)?));
    let actor = launch(
        &key,
        &runtime,
        &issuer,
        dir.path().join("cancel-hyper"),
        writable_host,
        &cancel_archive,
    )
    .await?;
    let waiting = data_dir.join("waiting");
    let resume = data_dir.join("resume");

    let admitted = {
        let actor = actor.clone();
        tokio::spawn(async move { actor.call(Block {}, 15_000).await })
    };
    wait_for_marker(&waiting).await?;
    let queued = {
        let actor = actor.clone();
        tokio::spawn(async move {
            actor
                .call(
                    Ping {
                        data: b"queued".to_vec(),
                    },
                    5_000,
                )
                .await
        })
    };
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    queued.abort();
    let _ = queued.await;
    fs::write(&resume, b"resume")?;
    admitted.await??;
    assert_eq!(
        actor
            .call(
                Ping {
                    data: b"ready".to_vec()
                },
                5_000
            )
            .await?
            .data,
        b"ready"
    );

    fs::remove_file(&resume)?;
    fs::remove_file(&waiting)?;
    let active = {
        let actor = actor.clone();
        tokio::spawn(async move { actor.call(Block {}, 15_000).await })
    };
    wait_for_marker(&waiting).await?;
    active.abort();
    let _ = active.await;
    fs::write(&resume, b"resume")?;
    assert_eq!(
        actor
            .call(
                Ping {
                    data: b"after local cancellation".to_vec()
                },
                5_000
            )
            .await?
            .data,
        b"after local cancellation"
    );

    let guest_id = actor.actor_id().clone();
    target
        .call_remote(
            guest_id.clone(),
            Ping {
                data: b"warm up".to_vec(),
            },
            10_000,
        )
        .await?;
    fs::remove_file(&resume)?;
    fs::remove_file(&waiting)?;
    let remote = {
        let caller = target.clone();
        let guest_id = guest_id.clone();
        tokio::spawn(async move { caller.call_remote(guest_id, Block {}, 2_000).await })
    };
    wait_for_marker(&waiting).await?;
    let timeout = remote.await?;
    assert!(
        matches!(
            timeout,
            Err(actr_protocol::ActrError::TimedOut
                | actr_protocol::ActrError::DeliveryUncertain { .. })
        ),
        "{timeout:?}"
    );
    fs::write(&resume, b"resume")?;
    assert_eq!(
        target
            .call_remote(
                guest_id,
                Ping {
                    data: b"same guest".to_vec()
                },
                5_000
            )
            .await?
            .data,
        b"same guest"
    );
    actor.shutdown();
    actor.wait_for_shutdown().await;
    assert_eq!(
        target
            .call(
                Ping {
                    data: b"independent".to_vec()
                },
                5_000
            )
            .await?
            .data,
        b"independent"
    );
    // Fault only the remaining test instance's private cgroup.
    let live = fs::read_dir(&host.state_dir)?.collect::<Result<Vec<_>, _>>()?;
    assert_eq!(live.len(), 1);
    let launch: serde_json::Value =
        serde_json::from_slice(&fs::read(live[0].path().join("launch.json"))?)?;
    let cgroup = PathBuf::from(launch["cgroup"].as_str().expect("owned test cgroup"));
    assert_eq!(cgroup.parent(), Some(host.cgroup_parent.as_path()));
    fs::write(cgroup.join("cgroup.kill"), b"1")?;
    tokio::time::timeout(
        std::time::Duration::from_secs(20),
        target.wait_for_shutdown(),
    )
    .await?;
    assert!(fs::read_dir(&host.state_dir)?.next().is_none());
    assert_eq!(fs::read(data_dir.join("start-marker"))?, b"started");
    signaling.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the provisioned Linux OCI test environment"]
async fn reports_guest_oom_during_startup_and_in_flight_rpcs() -> anyhow::Result<()> {
    use std::time::Duration;

    use actr_framework::{Context, MessageDispatcher, Workload, async_trait};
    use actr_protocol::{ActorResult, ActrError, ActrType, RpcEnvelope};
    use anyhow::Context as _;
    use bytes::Bytes;

    struct Client;

    #[async_trait]
    impl Workload for Client {
        type Dispatcher = Self;
    }

    #[async_trait]
    impl MessageDispatcher for Client {
        type Workload = Self;

        async fn dispatch<C: Context>(_: &Self, message: RpcEnvelope, _: &C) -> ActorResult<Bytes> {
            Err(ActrError::UnknownRoute(message.route_key))
        }
    }

    let layout =
        PathBuf::from(std::env::var_os("ACTR_OCI_TEST_LAYOUT").expect("ACTR_OCI_TEST_LAYOUT"));
    let settings =
        PathBuf::from(std::env::var_os("ACTR_OCI_TEST_SETTINGS").expect("ACTR_OCI_TEST_SETTINGS"));
    let mut host: OciRuntimeConfig = toml::from_str(&fs::read_to_string(settings)?)?;
    host.supervisor = PathBuf::from(env!("CARGO_BIN_EXE_actr-oci-supervisor"));
    host.allowed_capabilities = vec!["actor.receive".into()];
    host.memory_limit_bytes = 64 * 1024 * 1024;
    host.swap_limit_bytes = Some(0);

    let key = SigningKey::from_bytes(&[11; 32]);
    let dir = tempfile::tempdir()?;
    let mut signaling = TestSignalingServer::start_with_relay().await?;
    let issuer = format!("http://127.0.0.1:{}/issuer", signaling.port());
    let runtime = dir.path().join("actr.toml");
    let common = format!(
        "[signaling]\nurl={:?}\n[issuer_endpoint]\nurl={issuer:?}\n[deployment]\nrealm='test7@example.com'\n",
        signaling.url()
    );
    fs::write(&runtime, &common)?;
    let client_type = ActrType {
        publisher: "local".into(),
        name: "OciOomClient".into(),
        version: "1.0.0".into(),
    };
    let client_config = ConfigParser::from_runtime_file(
        &runtime,
        PackageInfo {
            name: client_type.name.clone(),
            actr_type: client_type.clone(),
            description: None,
            authors: Vec::new(),
            license: None,
        },
    )?;
    let client_hyper = Hyper::new(HyperConfig::new(
        dir.path().join("client"),
        Arc::new(StaticTrust::new(key.verifying_key().to_bytes())?),
    ))
    .await?;
    let client = Node::from_hyper(client_hyper, client_config)
        .link(Client)
        .await?
        .register(&issuer)
        .await?
        .start()
        .await?;
    fs::write(
        &runtime,
        format!(
            "{common}\n[[acl.rules]]\npermission='allow'\ntype={:?}\nrealm='self'\n",
            client_type.to_string_repr()
        ),
    )?;

    for (case, phase) in [
        ("local", "dispatch"),
        ("remote", "dispatch"),
        ("handshake", "handshake"),
        ("start", "start"),
    ] {
        let mut manifest =
            ActorPackageManifest::from_toml(&fs::read_to_string(layout.join("manifest.toml"))?)?;
        manifest
            .workload
            .oci
            .as_mut()
            .expect("OCI fixture")
            .env
            .insert("ACTR_ECHO_EXHAUST_MEMORY".into(), phase.into());
        let archive = dir.path().join(format!("{case}.actr"));
        pack_oci_package(
            manifest,
            &layout,
            &key,
            File::create(&archive)?,
            OciLimits::default(),
        )?;
        host.package_sha256 = hex::encode(Sha256::digest(fs::read(&archive)?));
        let actor = tokio::time::timeout(
            Duration::from_secs(10),
            launch(
                &key,
                &runtime,
                &issuer,
                dir.path().join(case),
                host.clone(),
                &archive,
            ),
        )
        .await
        .with_context(|| format!("waiting for {case} Guest startup"))?;
        if matches!(phase, "handshake" | "start") {
            let Err(error) = actor else {
                anyhow::bail!("OOM did not fail Guest startup during {phase}");
            };
            assert!(
                error.to_string().contains("killed by OOM"),
                "{phase}: {error:#}"
            );
            if phase == "handshake" {
                assert!(fs::read_dir(&host.state_dir)?.next().is_none());
            }
            continue;
        }
        let actor = actor?;
        if case == "remote" {
            let warmup = Ping {
                data: b"warm route".to_vec(),
            };
            let response = client
                .call_remote(actor.actor_id(), warmup.clone(), 12_000)
                .await
                .context("establishing the remote Guest route")?;
            assert_eq!(response.data, warmup.data);
        }
        let request = Block {};
        let result = tokio::time::timeout(Duration::from_secs(5), async {
            if case == "local" {
                actor.call(request, 12_000).await
            } else {
                client.call_remote(actor.actor_id(), request, 12_000).await
            }
        })
        .await
        .with_context(|| format!("waiting for OOM reply to {case} call"))?;
        assert!(
            matches!(&result, Err(ActrError::DeliveryUncertain { cause, .. }) if cause.contains("killed by OOM") && cause.contains("67108864")),
            "{case} caller did not receive the Guest OOM: {result:?}"
        );
        tokio::time::timeout(Duration::from_secs(15), actor.wait_for_shutdown())
            .await
            .with_context(|| format!("waiting for cleanup after {case} OOM"))?;
        assert!(fs::read_dir(&host.state_dir)?.next().is_none());
    }
    client.shutdown();
    client.wait_for_shutdown().await;
    signaling.shutdown().await;
    Ok(())
}

async fn launch(
    key: &SigningKey,
    runtime: &std::path::Path,
    issuer: &str,
    storage: PathBuf,
    config: OciRuntimeConfig,
    archive: &std::path::Path,
) -> anyhow::Result<actr_hyper::ActrRef> {
    let limits = OciLimits {
        archive_bytes: config.max_package_bytes,
        unpacked_layer_bytes: config.max_unpack_bytes,
    };
    let hyper = Hyper::new(HyperConfig::new(
        storage,
        Arc::new(StaticTrust::new(key.verifying_key().to_bytes())?),
    ))
    .await?
    .with_oci_runtime(config)?;
    let verified = hyper.verify_oci_package(archive, limits).await?;
    let actor = verified.manifest().actor_type();
    let runtime = ConfigParser::from_runtime_file(
        runtime,
        PackageInfo {
            name: actor.name.clone(),
            actr_type: actor,
            description: None,
            authors: Vec::new(),
            license: None,
        },
    )?;
    Ok(Node::from_hyper(hyper, runtime)
        .attach_verified_oci(verified)
        .await?
        .register(issuer)
        .await?
        .start()
        .await?)
}

async fn wait_for_marker(path: &std::path::Path) -> anyhow::Result<()> {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !path.exists() {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await?;
    Ok(())
}

#[cfg(feature = "dynclib-engine")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "explicit runtime comparison on the provisioned Linux OCI host"]
async fn compares_linked_dynclib_and_oci_echo() -> anyhow::Result<()> {
    use actr_framework::{Context, MessageDispatcher, Workload};
    use actr_protocol::{ActorResult, RpcEnvelope};
    use async_trait::async_trait;
    use bytes::Bytes;

    let Some(dylib) = std::env::var_os("ACTR_OCI_BENCH_DYLIB").filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    struct Echo;
    #[async_trait]
    impl Workload for Echo {
        type Dispatcher = Self;
    }
    #[async_trait]
    impl MessageDispatcher for Echo {
        type Workload = Self;
        async fn dispatch<C: Context>(
            _: &Self,
            envelope: RpcEnvelope,
            _: &C,
        ) -> ActorResult<Bytes> {
            Ok(envelope.payload.unwrap_or_default())
        }
    }

    let fixture =
        PathBuf::from(std::env::var_os("ACTR_OCI_TEST_LAYOUT").expect("ACTR_OCI_TEST_LAYOUT"));
    let settings =
        PathBuf::from(std::env::var_os("ACTR_OCI_TEST_SETTINGS").expect("ACTR_OCI_TEST_SETTINGS"));
    let mut host: OciRuntimeConfig = toml::from_str(&fs::read_to_string(settings)?)?;
    host.supervisor = PathBuf::from(env!("CARGO_BIN_EXE_actr-oci-supervisor"));
    host.allowed_capabilities = vec!["actor.receive".into()];
    let key = SigningKey::from_bytes(&[7; 32]);
    let dir = tempfile::tempdir()?;
    let mut signaling = TestSignalingServer::start().await?;
    let issuer = format!("http://127.0.0.1:{}/issuer", signaling.port());
    let runtime = dir.path().join("actr.toml");
    fs::write(
        &runtime,
        format!(
            "[signaling]\nurl={:?}\n[issuer_endpoint]\nurl={issuer:?}\n[deployment]\nrealm='test7@example.com'\n",
            signaling.url()
        ),
    )?;

    let original =
        ActorPackageManifest::from_toml(&fs::read_to_string(fixture.join("manifest.toml"))?)?;
    let archive = dir.path().join("oci.actr");
    pack_oci_package(
        original.clone(),
        &fixture,
        &key,
        File::create(&archive)?,
        OciLimits::default(),
    )?;
    host.package_sha256 = hex::encode(Sha256::digest(fs::read(&archive)?));

    for label in ["oci-cold", "oci-warm"] {
        let start = std::time::Instant::now();
        let actor = launch(
            &key,
            &runtime,
            &issuer,
            dir.path().join("oci"),
            host.clone(),
            &archive,
        )
        .await?;
        println!(
            "{label} readiness_ms={:.3}",
            start.elapsed().as_secs_f64() * 1000.0
        );
        measure_echo(label, &actor).await?;
        actor.shutdown();
        actor.wait_for_shutdown().await;
    }

    let actor = actr_protocol::ActrType {
        publisher: "local".into(),
        name: "BenchLinked".into(),
        version: "1.0.0".into(),
    };
    let linked_config = ConfigParser::from_runtime_file(
        &runtime,
        PackageInfo {
            name: actor.name.clone(),
            actr_type: actor,
            authors: Vec::new(),
            description: None,
            license: None,
        },
    )?;
    let start = std::time::Instant::now();
    let hyper = Hyper::new(HyperConfig::new(
        dir.path().join("linked"),
        Arc::new(StaticTrust::new(key.verifying_key().to_bytes())?),
    ))
    .await?;
    let linked = Node::from_hyper(hyper, linked_config)
        .link(Echo)
        .await?
        .register(&issuer)
        .await?
        .start()
        .await?;
    println!(
        "linked readiness_ms={:.3}",
        start.elapsed().as_secs_f64() * 1000.0
    );
    measure_echo("linked", &linked).await?;
    linked.shutdown();
    linked.wait_for_shutdown().await;

    let mut manifest = original;
    manifest.actor.name = "BenchDynClib".into();
    manifest.workload.profile = "actr.native/1".into();
    manifest.workload.entry = "echo.so".into();
    manifest.workload.oci = None;
    manifest.workload.rpc_contract = None;
    manifest.host.profile = "actr.host/1".into();
    manifest.host.required_capabilities.clear();
    manifest.files = vec![actr_pack::PackageFileEntry {
        path: "echo.so".into(),
        role: "workload".into(),
        media_type: "application/octet-stream".into(),
        size: 0,
        sha256: "0".repeat(64),
    }];
    let bytes = actr_pack::pack_actor_package(&actr_pack::ActorPackOptions {
        manifest,
        payloads: std::collections::BTreeMap::from([("echo.so".into(), fs::read(dylib)?)]),
        signing_key: key.clone(),
    })?;
    let package = actr_hyper::WorkloadPackage::new(bytes);
    let actor = package.manifest()?.actor_type();
    let config = ConfigParser::from_runtime_file(
        &runtime,
        PackageInfo {
            name: actor.name.clone(),
            actr_type: actor,
            authors: Vec::new(),
            description: None,
            license: None,
        },
    )?;
    let start = std::time::Instant::now();
    let hyper = Hyper::new(HyperConfig::new(
        dir.path().join("dynclib"),
        Arc::new(StaticTrust::new(key.verifying_key().to_bytes())?),
    ))
    .await?;
    let dylib = Node::from_hyper(hyper, config)
        .attach(&package)
        .await?
        .register(&issuer)
        .await?
        .start()
        .await?;
    println!(
        "dynclib readiness_ms={:.3}",
        start.elapsed().as_secs_f64() * 1000.0
    );
    measure_echo("dynclib", &dylib).await?;
    dylib.shutdown();
    dylib.wait_for_shutdown().await;
    signaling.shutdown().await;
    Ok(())
}

#[cfg(feature = "dynclib-engine")]
async fn measure_echo(label: &str, actor: &actr_hyper::ActrRef) -> anyhow::Result<()> {
    use actr_framework::Dest;
    use actr_protocol::PayloadType;
    use bytes::Bytes;
    use std::time::Instant;

    let context = actor.app_context().await;
    let data = Bytes::from_static(&[0; 64]);
    let mut latency = Vec::with_capacity(1000);
    let start = Instant::now();
    for _ in 0..1000 {
        let call = Instant::now();
        let result = context
            .call_raw(
                &Dest::Workload,
                "echo.Echo.Ping".into(),
                PayloadType::RpcReliable,
                data.clone(),
                5_000,
            )
            .await?;
        latency.push(call.elapsed().as_micros());
        assert_eq!(result, data);
    }
    let seconds = start.elapsed().as_secs_f64();
    latency.sort_unstable();
    println!(
        "{label} p50_us={} p95_us={} sequential_rpc_s={:.1}",
        latency[499],
        latency[949],
        1000.0 / seconds
    );
    Ok(())
}
