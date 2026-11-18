//! Fetch the archives named by a verified Realm Profile. No fetched code executes here.
use actr_pack::{
    ActorPackageManifest,
    realm::{check_realm_dependencies, check_realm_pair, verify_realm_archive},
};
use actr_protocol::{
    AuthorityDescriptor, KeyPinOutcome, KeyPins, PublisherKeys, RealmSigningKeys,
    realm_profile::{RealmPackage, RealmPackageSelection, RealmProfile, RealmStatus},
    uri::{ActrUri, is_forbidden_address},
};
use anyhow::{Context, Result, bail, ensure};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    fs::File,
    future::Future,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use tempfile::NamedTempFile;

use super::realm_members::resolve_profile;

const DOCUMENT_LIMIT: usize = actr_protocol::realm_profile::MAX_REALM_PROFILE_BYTES;
const ARCHIVE_LIMIT: usize = 64 * 1024 * 1024;

#[derive(Args, Debug)]
pub struct FetchArgs {
    /// Realm name, for example echo@realm.example
    #[arg(long)]
    pub realm: String,
    /// Additional archive references to download from the Realm package map
    #[arg(long = "package", value_name = "REF")]
    pub additional: Vec<String>,
    /// Realm-specific directory for verified archives and trust pins
    #[arg(long, value_name = "DIR")]
    pub output: PathBuf,
}

/// Validate manually selected archives before creating or updating a Realm.
#[derive(Args, Debug)]
pub struct CheckArgs {
    /// JSON file with primary_package, front_package and the complete packages map
    #[arg(long, value_name = "FILE")]
    pub selection: PathBuf,
    /// Directory for verified archives and Publisher trust pins
    #[arg(long, value_name = "DIR")]
    pub output: PathBuf,
}

#[derive(Default, Serialize, Deserialize)]
struct Cache {
    realm: String,
    realm_keys: Option<KeyPins>,
    profile: Option<String>,
    publishers: BTreeMap<String, KeyPins>,
    digests: BTreeMap<String, String>,
}

// Resolve and pin every target before connecting. This also applies to cross-origin redirects.
async fn download(address: &str, limit: usize) -> Result<Vec<u8>> {
    let mut url = url::Url::parse(address)?;
    for _ in 0..=5 {
        ensure!(
            url.scheme() == "https" && url.username().is_empty() && url.password().is_none(),
            "expected a credential-free HTTPS URL"
        );
        let host = url.host_str().context("URL has no host")?;
        let port = url.port_or_known_default().context("URL has no port")?;
        let addresses: Vec<_> = match url.host().context("URL has no host")? {
            url::Host::Domain(domain) => tokio::time::timeout(
                Duration::from_secs(5),
                tokio::net::lookup_host((domain, port)),
            )
            .await??
            .collect(),
            url::Host::Ipv4(ip) => vec![(std::net::IpAddr::V4(ip), port).into()],
            url::Host::Ipv6(ip) => vec![(std::net::IpAddr::V6(ip), port).into()],
        };
        ensure!(
            !addresses.is_empty() && addresses.iter().all(|a| !is_forbidden_address(a.ip())),
            "package discovery requires a public network address"
        );
        let client = reqwest::Client::builder()
            .https_only(true)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .resolve_to_addrs(host, &addresses)
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()?;
        let mut response = client.get(url.clone()).send().await?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .context("redirect missing Location")?
                .to_str()?;
            url = url.join(location)?;
            continue;
        }
        ensure!(
            response.status() == reqwest::StatusCode::OK,
            "download failed with {}",
            response.status()
        );
        ensure!(
            response.content_length().is_none_or(|n| n <= limit as u64),
            "download exceeds size limit"
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            ensure!(
                chunk.len() <= limit - bytes.len(),
                "download exceeds size limit"
            );
            bytes.extend_from_slice(&chunk);
        }
        return Ok(bytes);
    }
    bail!("too many redirects")
}

fn publisher_keys(
    domain: &str,
    uri: &ActrUri,
    doc: &[u8],
    cache: &mut Cache,
) -> Result<PublisherKeys> {
    let value = actr_protocol::signed_json::parse(doc)?;
    let authority: AuthorityDescriptor = serde_json::from_value(value)?;
    authority.validate_for(uri)?;
    let publisher = authority.publisher.context("missing Publisher Profile")?;
    let (keys, pins) =
        publisher.accept(domain, chrono::Utc::now(), cache.publishers.get(domain))?;
    let KeyPinOutcome::Accepted(pins) = pins else {
        bail!("Publisher identity changed for {domain}");
    };
    cache.publishers.insert(domain.to_owned(), pins);
    Ok(keys)
}

async fn archive<F, Fut>(
    reference: &RealmPackage,
    keys: &PublisherKeys,
    path: &Path,
    get: &F,
) -> Result<(ActorPackageManifest, Vec<u8>)>
where
    F: Fn(String, usize) -> Fut,
    Fut: Future<Output = Result<Vec<u8>>>,
{
    let bytes = match File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(ARCHIVE_LIMIT as u64 + 1)
                .read_to_end(&mut bytes)?;
            bytes
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            get(reference.href.clone(), ARCHIVE_LIMIT).await?
        }
        Err(e) => return Err(e.into()),
    };
    let verified = verify_realm_archive(reference, &bytes, &keys.keys, ARCHIVE_LIMIT)?;
    Ok((verified.manifest, bytes))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut temp = NamedTempFile::new_in(path.parent().context("missing cache directory")?)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path)?;
    Ok(())
}

impl Cache {
    fn pin_package_digests(&mut self, profile: &RealmProfile) -> Result<()> {
        if let Some(selection) = &profile.selection {
            for package in selection.packages.values() {
                let pinned = self
                    .digests
                    .entry(package.r#type.clone())
                    .or_insert_with(|| package.sha256.clone());
                ensure!(
                    *pinned == package.sha256,
                    "PACKAGE_EQUIVOCATION for {}",
                    package.r#type
                );
            }
        }
        Ok(())
    }

    fn accept_profile(
        &mut self,
        next: &RealmProfile,
        keys: &RealmSigningKeys,
        pins: KeyPins,
        path: &Path,
    ) -> Result<()> {
        if let Some(old) = &self.profile {
            // Removing a Realm signing key also removes the baseline it signed.
            let value = actr_protocol::signed_json::parse(old.as_bytes())?;
            if keys
                .keys
                .iter()
                .any(|k| value["signature"]["key_id"].as_str() == Some(&k.id))
            {
                let previous = RealmProfile::verify(old.as_bytes(), &self.realm, keys)?;
                previous.check_successor(next)?;
                // Older caches pinned only downloaded archives. Recover the rest before replacing the Profile.
                self.pin_package_digests(&previous)?;
            }
        }
        self.pin_package_digests(next)?;
        self.realm_keys = Some(pins);
        self.profile = Some(std::str::from_utf8(next.document())?.to_owned());
        write_atomic(path, &serde_json::to_vec(self)?)
    }
}

pub async fn fetch(args: &FetchArgs) -> Result<Value> {
    fetch_with(
        args,
        |url, limit| async move { download(&url, limit).await },
    )
    .await
}

pub(super) async fn fetch_with<F, Fut>(args: &FetchArgs, get: F) -> Result<Value>
where
    F: Fn(String, usize) -> Fut,
    Fut: Future<Output = Result<Vec<u8>>>,
{
    std::fs::create_dir_all(&args.output)?;
    let lock = File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(args.output.join(".lock"))?;
    let _lock = tokio::task::spawn_blocking(move || {
        lock.lock()?;
        std::io::Result::Ok(lock)
    })
    .await??;
    let path = args.output.join("state.json");
    let mut cache: Cache = match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Cache {
            realm: args.realm.clone(),
            ..Default::default()
        },
        Err(e) => return Err(e.into()),
    };
    ensure!(
        cache.realm == args.realm,
        "output directory belongs to a different Realm"
    );
    let (profile, keys, pins) = resolve_profile(
        &args.realm,
        None,
        |url| get(url, DOCUMENT_LIMIT),
        cache.realm_keys.as_ref(),
    )
    .await?;
    cache.accept_profile(&profile, &keys, pins, &path)?;
    if profile.realm.status == RealmStatus::Retired {
        bail!("Realm {} is retired", args.realm);
    }
    let selection = profile
        .selection
        .as_ref()
        .context("missing active selection")?;
    let roots = [&selection.primary_package, &selection.front_package]
        .into_iter()
        .chain(&args.additional)
        .cloned()
        .collect();
    let mut result =
        prepare_selection(selection, &args.output, &mut cache, &path, roots, &get).await?;
    result["realm"] = json!(args.realm);
    result["sequence"] = json!(profile.sequence);
    Ok(result)
}

pub async fn check_selection(args: &CheckArgs) -> Result<Value> {
    check_selection_with(
        args,
        |url, limit| async move { download(&url, limit).await },
    )
    .await
}

pub(super) async fn check_selection_with<F, Fut>(args: &CheckArgs, get: F) -> Result<Value>
where
    F: Fn(String, usize) -> Fut,
    Fut: Future<Output = Result<Vec<u8>>>,
{
    let selection = super::realm::read_selection(&args.selection)?;
    std::fs::create_dir_all(&args.output)?;
    let lock = File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(args.output.join(".lock"))?;
    let _lock = tokio::task::spawn_blocking(move || {
        lock.lock()?;
        std::io::Result::Ok(lock)
    })
    .await??;
    let path = args.output.join("selection-state.json");
    let mut cache: Cache = match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Cache::default(),
        Err(error) => return Err(error.into()),
    };
    let roots = selection.packages.keys().cloned().collect();
    let mut result =
        prepare_selection(&selection, &args.output, &mut cache, &path, roots, &get).await?;
    result["selection"] = serde_json::to_value(&selection)?;
    Ok(result)
}

async fn prepare_selection<F, Fut>(
    selection: &RealmPackageSelection,
    output: &Path,
    cache: &mut Cache,
    path: &Path,
    roots: Vec<String>,
    get: &F,
) -> Result<Value>
where
    F: Fn(String, usize) -> Fut,
    Fut: Future<Output = Result<Vec<u8>>>,
{
    let mut publishers = BTreeMap::new();
    let mut privacy_contract = None;
    let mut manifests = BTreeMap::new();
    let mut files = BTreeMap::new();
    let mut locks = BTreeMap::new();
    let mut queue = VecDeque::from(roots);
    while let Some(name) = queue.pop_front() {
        if files.contains_key(&name) {
            continue;
        }
        let entry = selection
            .packages
            .get(&name)
            .with_context(|| format!("unknown package reference {name:?}"))?;
        let domain = entry.actr_type().map_err(anyhow::Error::msg)?.publisher;
        let keys = match publishers.entry(domain.clone()) {
            std::collections::btree_map::Entry::Occupied(slot) => slot.into_mut(),
            std::collections::btree_map::Entry::Vacant(slot) => {
                let uri = ActrUri::from_domain(&domain)?;
                let document = get(
                    uri.discovery_url().context("Publisher URL")?,
                    DOCUMENT_LIMIT,
                )
                .await?;
                let keys = publisher_keys(&domain, &uri, &document, cache)?;
                write_atomic(path, &serde_json::to_vec(&cache)?)?;
                slot.insert(keys)
            }
        };
        let destination = output.join(format!("{}.actr", entry.sha256));
        let (manifest, bytes) = archive(entry, keys, &destination, &get).await?;
        if name == selection.primary_package {
            privacy_contract =
                actr_pack::realm::privacy_contract_digest(&manifest, &bytes, ARCHIVE_LIMIT)?;
        }
        if manifest
            .files
            .iter()
            .any(|file| file.role == "lock" || file.path == "manifest.lock.toml")
        {
            let lock = actr_hyper::verified_package_lock(&bytes, &manifest, ARCHIVE_LIMIT)?
                .context("missing declared package lock")?;
            locks.insert(
                name.clone(),
                lock.dependencies
                    .iter()
                    .map(|entry| (entry.alias.clone(), entry.actr_type.clone()))
                    .collect(),
            );
            if name == selection.front_package {
                let locked = lock
                    .get_dependency_by_alias("service")
                    .context("signed Front lock does not select a Service")?;
                ensure!(
                    locked.actr_type == selection.selected().map_err(anyhow::Error::msg)?.0.r#type,
                    "signed Front lock pins a different Service"
                );
            }
        }
        if !destination.exists() {
            write_atomic(&destination, &bytes)?;
        }
        for target in entry.dependencies.values() {
            if !files.contains_key(target) {
                queue.push_back(target.clone());
            }
        }
        manifests.insert(name.clone(), manifest);
        files.insert(name.clone(), destination);
    }
    check_realm_dependencies(selection, &manifests, &locks)?;
    check_realm_pair(
        &manifests[&selection.primary_package],
        &manifests[&selection.front_package],
    )?;
    let dependencies: Vec<_> = manifests.iter().flat_map(|(source, manifest)| {
        manifest.dependencies.iter().map(move |(alias, declared)| {
            let target = selection.packages[source].dependencies.get(alias);
            json!({"source":source,"alias":alias,"required":declared.required,"target":target,"status":if target.is_some(){"selected"}else{"unspecified"}})
        })
    }).collect();
    Ok(
        json!({"primary_package":selection.primary_package,"front_package":selection.front_package,"service":files[&selection.primary_package],"front":files[&selection.front_package],"packages":files,"dependencies":dependencies,"privacy_contract_sha256":privacy_contract}),
    )
}

#[cfg(test)]
mod tests;
