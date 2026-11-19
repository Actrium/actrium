use actr_protocol::realm_membership::{MembershipOp, MembershipRequest};
use actr_protocol::realm_profile::{FrontUpdate, PackageSelectionChange};

use super::*;
use crate::commands::realm_members::{Member, SendError};

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
) -> Result<Value> {
    ensure!(
        member.file.realm.rsplit_once('@').map(|(_, domain)| domain)
            == Some(member.file.provider_id.as_str()),
        "this command requires a Realm under its Provider domain"
    );
    release_with(
        &member.file.realm,
        &member.file.provider_id,
        output,
        requested,
        &|url, limit| async move { download(&url, limit).await },
        |request| member.send_with_outcome(request),
    )
    .await
}

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
    Reply: Future<Output = std::result::Result<Value, SendError>>,
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
        prepare_selection(
            &selection,
            output,
            &mut cache,
            &state_path,
            selection.packages.keys().cloned().collect(),
            get,
        )
        .await?;
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
    let output = match send(request).await {
        Ok(output) => output,
        Err(error) => {
            let context = format!("package release {}", pending.update.operation_id);
            if matches!(&error, SendError::Refused(_)) {
                journal.pending = None;
                write_atomic(&path, &serde_json::to_vec(&journal)?)
                    .context("clear the rejected package release")?;
            }
            return Err(anyhow::Error::from(error)).context(context);
        }
    };
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
