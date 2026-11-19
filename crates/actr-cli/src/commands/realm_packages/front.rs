use actr_protocol::realm_membership::{MembershipOp, MembershipRequest};
use actr_protocol::realm_profile::{FrontUpdate, PackageSelectionChange};
use base64::Engine as _;

use super::*;
use crate::commands::realm_members::Member;

#[derive(Args, Debug, Default)]
pub struct ServiceProbeArgs {
    /// Runtime config for the probe Front, using a Realm member key.
    #[arg(long, value_name = "FILE", requires = "probe_route")]
    pub probe_config: Option<PathBuf>,
    /// A read-only RPC that succeeds only when the target is ready.
    #[arg(long, requires = "probe_config")]
    pub probe_route: Option<String>,
    /// Optional encoded request body. The default is empty.
    #[arg(long, value_name = "FILE", requires = "probe_config")]
    pub probe_request: Option<PathBuf>,
}

impl ServiceProbeArgs {
    pub fn probe(&self) -> Option<ServiceProbe> {
        Some(ServiceProbe {
            probe_config: self.probe_config.clone()?,
            probe_route: self.probe_route.clone()?,
            probe_request: self.probe_request.clone(),
            probe_payload: None,
        })
    }
}

pub struct ServiceProbe {
    pub probe_config: PathBuf,
    pub probe_route: String,
    pub probe_request: Option<PathBuf>,
    pub probe_payload: Option<Vec<u8>>,
}

pub fn read_release_selection(
    path: &Path,
) -> Result<(RealmPackageSelection, Option<ServiceProbe>)> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Readiness {
        config: PathBuf,
        route: String,
        request_base64: Option<String>,
    }
    let bytes = std::fs::read(path)?;
    ensure!(
        bytes.len() <= 2 * 1024 * 1024,
        "selection and readiness JSON exceeds 2 MiB"
    );
    let document = actr_protocol::signed_json::parse(&bytes)?;
    let selection: RealmPackageSelection = serde_json::from_value(document.clone())?;
    selection.check().map_err(anyhow::Error::msg)?;
    ensure!(
        serde_json::to_vec(&selection)?.len() <= DOCUMENT_LIMIT,
        "package selection exceeds 64 KiB"
    );
    let readiness = document
        .get("readiness")
        .map(|value| -> Result<ServiceProbe> {
            let readiness: Readiness = serde_json::from_value(value.clone())?;
            let probe_payload = readiness
                .request_base64
                .as_deref()
                .map(|body| base64::engine::general_purpose::STANDARD.decode(body))
                .transpose()
                .context("invalid readiness.request_base64")?;
            Ok(ServiceProbe {
                probe_config: path
                    .parent()
                    .context("selection path has no parent")?
                    .join(readiness.config),
                probe_route: readiness.route,
                probe_request: None,
                probe_payload,
            })
        })
        .transpose()?;
    Ok((selection, readiness))
}

#[derive(Serialize, Deserialize)]
struct Release {
    update: FrontUpdate,
    selection: RealmPackageSelection,
    rollback: bool,
    #[serde(default)]
    kind: Option<PackageSelectionChange>,
}

#[derive(Deserialize)]
struct Receipt {
    operation_id: String,
    sequence: u64,
    previous_selection: RealmPackageSelection,
}

#[derive(Default, Serialize, Deserialize)]
struct Journal {
    pending: Option<Release>,
    previous: Option<RealmPackageSelection>,
    current: Option<RealmPackageSelection>,
}

fn load_journal(path: &Path, current: &RealmPackageSelection) -> Result<Journal> {
    match std::fs::read(path) {
        Ok(bytes) => return Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let mut candidates = Vec::new();
    for (name, kind) in [
        ("front-release.json", PackageSelectionChange::Front),
        ("service-release.json", PackageSelectionChange::Service),
    ] {
        let legacy = path.with_file_name(name);
        let bytes = match std::fs::read(&legacy) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        let mut journal: Journal = serde_json::from_slice(&bytes)?;
        if let Some(pending) = &mut journal.pending {
            pending.kind = Some(kind);
            pending.update.rollback = pending.rollback;
        }
        if journal.pending.is_some() || journal.current.as_ref() == Some(current) {
            candidates.push((
                journal.pending.is_some(),
                std::fs::metadata(&legacy)?.modified()?,
                journal,
            ));
        }
    }
    Ok(candidates
        .into_iter()
        .max_by_key(|(pending, modified, _)| (*pending, *modified))
        .map_or_else(Journal::default, |(_, _, journal)| journal))
}

pub async fn release(
    member: &Member,
    output: &Path,
    requested: Option<RealmPackageSelection>,
    service: Option<&ServiceProbe>,
) -> Result<Value> {
    ensure!(
        member.file.realm.rsplit_once('@').map(|(_, domain)| domain)
            == Some(member.file.provider_id.as_str()),
        "this command requires a Realm under its Provider domain"
    );
    release_selection_with(
        &member.file.realm,
        &member.file.provider_id,
        output,
        requested,
        service,
        &|url, limit| async move { download(&url, limit).await },
        |request| member.send(request),
    )
    .await
}

#[cfg(test)]
pub(super) async fn release_with<F, Fut, S, Reply>(
    realm: &str,
    provider: &str,
    output: &Path,
    requested: Option<RealmPackageSelection>,
    get: &F,
    send: S,
) -> Result<Value>
where
    F: Fn(String, usize) -> Fut,
    Fut: Future<Output = Result<Vec<u8>>>,
    S: FnOnce(MembershipRequest) -> Reply,
    Reply: Future<Output = Result<Value>>,
{
    release_selection_with(realm, provider, output, requested, None, get, send).await
}

pub(super) async fn release_selection_with<F, Fut, S, Reply>(
    realm: &str,
    provider: &str,
    output: &Path,
    requested: Option<RealmPackageSelection>,
    service: Option<&ServiceProbe>,
    get: &F,
    send: S,
) -> Result<Value>
where
    F: Fn(String, usize) -> Fut,
    Fut: Future<Output = Result<Vec<u8>>>,
    S: FnOnce(MembershipRequest) -> Reply,
    Reply: Future<Output = Result<Value>>,
{
    std::fs::create_dir_all(output)?;
    let lock = File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(output.join(".lock"))?;
    let _lock = tokio::task::spawn_blocking(move || {
        lock.lock()?;
        std::io::Result::Ok(lock)
    })
    .await??;
    let state_path = output.join("state.json");
    let mut cache: Cache = match std::fs::read(&state_path) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Cache {
            realm: realm.to_owned(),
            ..Default::default()
        },
        Err(error) => return Err(error.into()),
    };
    ensure!(
        cache.realm == realm,
        "output directory belongs to a different Realm"
    );
    let path = output.join("package-release.json");
    let (profile, keys, pins) = resolve_profile(
        realm,
        None,
        |url| get(url, DOCUMENT_LIMIT),
        cache.realm_keys.as_ref(),
    )
    .await?;
    cache.accept_profile(&profile, &keys, pins, &state_path)?;
    ensure!(
        profile.provider.id == provider,
        "Realm Profile names a different Provider"
    );
    ensure!(
        profile.realm.status == RealmStatus::Active,
        "Realm is retired"
    );
    let current = profile
        .selection
        .as_ref()
        .context("missing active selection")?;
    let mut journal = load_journal(&path, current)?;

    if let Some(pending) = &journal.pending
        && profile.sequence > pending.update.expected_sequence
        && current != &pending.selection
    {
        journal.pending = None;
        write_atomic(&path, &serde_json::to_vec(&journal)?)?;
        bail!("the pending package release was superseded; review the current selection and retry");
    }
    let rollback = requested.is_none();
    let selection = if let Some(pending) = &journal.pending {
        ensure!(
            pending.rollback == rollback
                && requested.as_ref().is_none_or(|s| *s == pending.selection),
            "package release {} is pending; retry the same command and selection",
            pending.update.operation_id
        );
        pending.selection.clone()
    } else if let Some(requested) = requested {
        requested
    } else {
        ensure!(
            journal.current.as_ref() == Some(current),
            "the current selection no longer matches the last release"
        );
        journal
            .previous
            .clone()
            .context("no previous selection in this directory")?
    };
    let kind = if let Some(pending) = &journal.pending {
        pending
            .kind
            .context("release journal is missing the change type")?
    } else if let Some(change) = current
        .release_change(&selection)
        .map_err(anyhow::Error::msg)?
    {
        change
    } else {
        return Ok(json!({"realm": realm, "sequence": profile.sequence, "changed": false}));
    };
    let service_change = kind == PackageSelectionChange::Service;
    for package in selection.packages.values() {
        ensure!(
            cache
                .digests
                .get(&package.r#type)
                .is_none_or(|old| *old == package.sha256),
            "PACKAGE_EQUIVOCATION for {}",
            package.r#type
        );
    }
    let new_operation = journal.pending.is_none();
    if new_operation
        || (service_change
            && journal
                .pending
                .as_ref()
                .is_some_and(|pending| pending.update.expected_sequence == profile.sequence))
    {
        let probe = if service_change {
            Some(service.context(
                "Service change requires readiness in the JSON or --probe-config and --probe-route",
            )?)
        } else {
            None
        };
        let verified = prepare_selection(
            &selection,
            output,
            &mut cache,
            &state_path,
            selection.packages.keys().cloned().collect(),
            get,
        )
        .await?;
        if let Some(probe) = probe {
            probe.check_ready(&profile, &selection, &verified).await?;
        }
    }
    if new_operation {
        journal.pending = Some(Release {
            update: FrontUpdate {
                operation_id: hex::encode(rand::random::<[u8; 16]>()),
                expected_sequence: profile.sequence,
                rollback,
            },
            selection,
            rollback,
            kind: Some(kind),
        });
        write_atomic(&path, &serde_json::to_vec(&journal)?)?;
    }
    let pending = journal
        .pending
        .as_ref()
        .context("missing planned package release")?;
    let mut request = MembershipRequest::new(
        if service_change {
            MembershipOp::SwitchService
        } else {
            MembershipOp::UpdateFront
        },
        realm,
        provider,
        &hex::encode(rand::random::<[u8; 16]>()),
    );
    request.selection = Some(pending.selection.clone());
    if service_change {
        request.service_update = Some(pending.update.clone());
    } else {
        request.front_update = Some(pending.update.clone());
    }
    tracing::info!(
        realm,
        operation_id = pending.update.operation_id,
        base = pending.update.expected_sequence,
        rollback,
        ?kind,
        "publishing package selection"
    );
    let output = send(request)
        .await
        .with_context(|| format!("package release {}", pending.update.operation_id))?;
    let receipt: Receipt =
        serde_json::from_value(output.clone()).context("invalid package release receipt")?;
    ensure!(
        receipt.operation_id == pending.update.operation_id
            && receipt.sequence == pending.update.expected_sequence + 1,
        "package release receipt does not match the pending operation"
    );
    let (published, keys, pins) = resolve_profile(
        realm,
        None,
        |url| get(url, DOCUMENT_LIMIT),
        cache.realm_keys.as_ref(),
    )
    .await?;
    cache.accept_profile(&published, &keys, pins, &state_path)?;
    if published.sequence > receipt.sequence
        && published.selection.as_ref() != Some(&pending.selection)
    {
        journal.pending = None;
        write_atomic(&path, &serde_json::to_vec(&journal)?)?;
        bail!(
            "package release {} was accepted, then superseded",
            receipt.operation_id
        );
    }
    ensure!(
        published.sequence >= receipt.sequence
            && published.selection.as_ref() == Some(&pending.selection),
        "package release {} was accepted at sequence {}, but the public selection is different; retry the same command",
        receipt.operation_id,
        receipt.sequence
    );
    journal.current = Some(pending.selection.clone());
    journal.previous = (!rollback).then_some(receipt.previous_selection);
    journal.pending = None;
    write_atomic(&path, &serde_json::to_vec(&journal)?)?;
    tracing::info!(
        realm,
        operation_id = receipt.operation_id,
        sequence = published.sequence,
        ?kind,
        "package selection verified at public address"
    );
    Ok(output)
}

impl ServiceProbe {
    pub(super) async fn check_ready(
        &self,
        profile: &RealmProfile,
        selection: &RealmPackageSelection,
        verified: &Value,
    ) -> Result<()> {
        use actr_hyper::{Dest, Node, WorkloadPackage};
        let service_type = selection
            .selected()
            .map_err(anyhow::Error::msg)?
            .0
            .actr_type()
            .map_err(anyhow::Error::msg)?;
        ensure!(
            !self.probe_route.trim().is_empty(),
            "readiness probe route is empty"
        );
        let mut payload = self.probe_payload.clone().unwrap_or_default();
        if let Some(path) = &self.probe_request {
            File::open(path)?
                .take(1024 * 1024 + 1)
                .read_to_end(&mut payload)?;
        }
        ensure!(
            payload.len() <= 1024 * 1024,
            "readiness probe request exceeds 1 MiB"
        );
        let archive = std::fs::read(
            verified["front"]
                .as_str()
                .context("missing verified Front")?,
        )?;
        let node = Node::from_config_file(&self.probe_config).await?;
        let config = node.runtime_config();
        ensure!(
            config.realm == profile.realm.name
                && config.issuer_endpoint == profile.issuer.url
                && profile.provider.signaling.iter().any(|endpoint| {
                    url::Url::parse(&endpoint.url).is_ok_and(|url| url == config.signaling_url)
                }),
            "probe config must use the Realm's verified Issuer and signaling"
        );
        let probe = tokio::time::timeout(Duration::from_secs(30), async {
            let registered = node
                .attach_front(&WorkloadPackage::new(archive), &service_type)
                .await?
                .register(&profile.issuer.url)
                .await?;
            Ok::<_, anyhow::Error>(registered.start().await?)
        })
        .await
        .context("starting the readiness probe")??;
        let response = async {
            let target = tokio::time::timeout(
                Duration::from_secs(10),
                probe.discover_service(&service_type),
            )
            .await
            .context("discovering the target Service")?
            .with_context(|| format!("could not find one Service of type {service_type}"))?;
            probe
                .app_context()
                .await
                .call_raw(
                    &Dest::Peer(target.clone()),
                    self.probe_route.clone(),
                    actr_protocol::PayloadType::RpcReliable,
                    payload.into(),
                    10_000,
                )
                .await
                .with_context(|| {
                    format!("Service {service_type} did not pass the readiness RPC")
                })?;
            tracing::info!(actor_id = %target, %service_type, "Service readiness RPC passed");
            Ok(())
        }
        .await;
        probe.shutdown();
        probe.wait_for_shutdown().await;
        response
    }
}
