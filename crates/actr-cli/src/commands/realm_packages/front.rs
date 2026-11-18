use actr_protocol::realm_membership::{MembershipOp, MembershipRequest};
use actr_protocol::realm_profile::FrontUpdate;

use super::*;
use crate::commands::realm::read_selection;
use crate::commands::realm_members::{KeyArgs, Member};

#[derive(Args, Debug)]
pub struct FrontRollbackArgs {
    #[command(flatten)]
    pub key: KeyArgs,
    /// Persistent directory for this Realm's Front release history and verified packages.
    #[arg(long, value_name = "DIR")]
    pub output: PathBuf,
}

#[derive(Args, Debug)]
pub struct FrontUpdateArgs {
    #[command(flatten)]
    pub release: FrontRollbackArgs,
    /// Complete replacement package selection. Service and its dependencies must match.
    #[arg(long, value_name = "FILE")]
    pub selection: PathBuf,
}

#[derive(Args, Debug)]
pub struct ServiceProbe {
    /// Canonical ActorId of the running target Service.
    #[arg(long)]
    pub actor_id: String,
    /// Runtime config for the probe Front, using a Realm member key.
    #[arg(long, value_name = "FILE")]
    pub probe_config: PathBuf,
    /// A read-only RPC that succeeds only when the target is ready.
    #[arg(long)]
    pub probe_route: String,
    /// Optional encoded request body. The default is empty.
    #[arg(long, value_name = "FILE")]
    pub probe_request: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct ServiceRollbackArgs {
    #[command(flatten)]
    pub release: FrontRollbackArgs,
    #[command(flatten)]
    pub probe: ServiceProbe,
}

#[derive(Args, Debug)]
pub struct ServiceSwitchArgs {
    #[command(flatten)]
    pub rollback: ServiceRollbackArgs,
    /// Complete selection for the target Service and a compatible Front.
    #[arg(long, value_name = "FILE")]
    pub selection: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct Release {
    update: FrontUpdate,
    selection: RealmPackageSelection,
    rollback: bool,
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

pub async fn update_front(args: &FrontUpdateArgs) -> Result<Value> {
    release(&args.release, Some(read_selection(&args.selection)?), None).await
}

pub async fn rollback_front(args: &FrontRollbackArgs) -> Result<Value> {
    release(args, None, None).await
}

pub async fn switch_service(args: &ServiceSwitchArgs) -> Result<Value> {
    release(
        &args.rollback.release,
        Some(read_selection(&args.selection)?),
        Some(&args.rollback.probe),
    )
    .await
}

pub async fn rollback_service(args: &ServiceRollbackArgs) -> Result<Value> {
    release(&args.release, None, Some(&args.probe)).await
}

async fn release(
    args: &FrontRollbackArgs,
    requested: Option<RealmPackageSelection>,
    service: Option<&ServiceProbe>,
) -> Result<Value> {
    let member = Member::load(&args.key)?;
    ensure!(
        member.file.realm.rsplit_once('@').map(|(_, domain)| domain)
            == Some(member.file.provider_id.as_str()),
        "this command requires a Realm under its Provider domain"
    );
    release_selection_with(
        &member.file.realm,
        &member.file.provider_id,
        &args.output,
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
    let kind = if service.is_some() {
        "service"
    } else {
        "front"
    };
    let path = output.join(format!("{kind}-release.json"));
    let mut journal: Journal = match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Journal::default(),
        Err(error) => return Err(error.into()),
    };
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

    if let Some(pending) = &journal.pending
        && profile.sequence > pending.update.expected_sequence
        && current != &pending.selection
    {
        journal.pending = None;
        write_atomic(&path, &serde_json::to_vec(&journal)?)?;
        bail!(
            "the pending {kind} operation was superseded; review the current selection and retry"
        );
    }
    let requested = requested.map(|mut target| {
        if let Some(probe) = service {
            target.service_actor_id = Some(probe.actor_id.clone());
        } else if target.service_actor_id.is_none() {
            target.service_actor_id = current.service_actor_id.clone();
        }
        target
    });
    let rollback = requested.is_none();
    let mut selection = if let Some(pending) = &journal.pending {
        ensure!(
            pending.rollback == rollback
                && requested.as_ref().is_none_or(|s| *s == pending.selection)
                && service.is_none_or(|probe| pending.selection.service_actor_id.as_deref()
                    == Some(probe.actor_id.as_str())),
            "{kind} operation {} is pending; retry the same command and selection",
            pending.update.operation_id
        );
        pending.selection.clone()
    } else if let Some(requested) = requested {
        requested
    } else {
        ensure!(
            journal.current.as_ref() == Some(current),
            "the current selection no longer matches the last {kind} release"
        );
        journal
            .previous
            .clone()
            .with_context(|| format!("no previous {kind} selection in this directory"))?
    };
    if let Some(probe) = service {
        selection.service_actor_id = Some(probe.actor_id.clone());
        selection.service_actor(realm).map_err(anyhow::Error::msg)?;
    } else {
        current
            .check_unchanged_service(&selection)
            .map_err(anyhow::Error::msg)?;
    }
    current
        .check_successor(&selection)
        .map_err(anyhow::Error::msg)?;
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
    if new_operation {
        ensure!(
            selection != *current,
            "package selection is already current"
        );
    }
    if new_operation
        || (service.is_some()
            && journal
                .pending
                .as_ref()
                .is_some_and(|pending| pending.update.expected_sequence == profile.sequence))
    {
        let verified = prepare_selection(
            &selection,
            output,
            &mut cache,
            &state_path,
            selection.packages.keys().cloned().collect(),
            get,
        )
        .await?;
        if let Some(probe) = service {
            probe.check_ready(&profile, &selection, &verified).await?;
        }
    }
    if new_operation {
        journal.pending = Some(Release {
            update: FrontUpdate {
                operation_id: hex::encode(rand::random::<[u8; 16]>()),
                expected_sequence: profile.sequence,
            },
            selection,
            rollback,
        });
        write_atomic(&path, &serde_json::to_vec(&journal)?)?;
    }
    let pending = journal
        .pending
        .as_ref()
        .context("missing planned package release")?;
    let mut request = MembershipRequest::new(
        if service.is_some() {
            MembershipOp::SwitchService
        } else {
            MembershipOp::UpdateFront
        },
        realm,
        provider,
        &hex::encode(rand::random::<[u8; 16]>()),
    );
    request.selection = Some(pending.selection.clone());
    if service.is_some() {
        request.service_update = Some(pending.update.clone());
    } else {
        request.front_update = Some(pending.update.clone());
    }
    tracing::info!(
        realm,
        operation_id = pending.update.operation_id,
        base = pending.update.expected_sequence,
        rollback,
        kind,
        "publishing package selection"
    );
    let output = send(request)
        .await
        .with_context(|| format!("{kind} operation {}", pending.update.operation_id))?;
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
            "{kind} operation {} was accepted, then superseded",
            receipt.operation_id
        );
    }
    ensure!(
        published.sequence >= receipt.sequence
            && published.selection.as_ref() == Some(&pending.selection),
        "{kind} operation {} was accepted at sequence {}, but the public selection is different; retry the same command",
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
        kind,
        "package selection verified at public address"
    );
    Ok(output)
}

impl ServiceProbe {
    async fn check_ready(
        &self,
        profile: &RealmProfile,
        selection: &RealmPackageSelection,
        verified: &Value,
    ) -> Result<()> {
        use actr_hyper::{Dest, Node, WorkloadPackage};
        let target = selection
            .service_actor(&profile.realm.name)
            .map_err(anyhow::Error::msg)?
            .context("missing Service ActorId")?;
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
        let mut payload = Vec::new();
        if let Some(path) = &self.probe_request {
            File::open(path)?
                .take(1024 * 1024 + 1)
                .read_to_end(&mut payload)?;
            ensure!(
                payload.len() <= 1024 * 1024,
                "readiness probe request exceeds 1 MiB"
            );
        }
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
        let response = probe
            .app_context()
            .await
            .call_raw(
                &Dest::Peer(target.clone()),
                self.probe_route.clone(),
                actr_protocol::PayloadType::RpcReliable,
                payload.into(),
                10_000,
            )
            .await;
        probe.shutdown();
        probe.wait_for_shutdown().await;
        response.with_context(|| format!("Service {target} did not pass the readiness RPC"))?;
        tracing::info!(actor_id = %target, %service_type, "Service readiness RPC passed");
        Ok(())
    }
}
