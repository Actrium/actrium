use super::*;
use crate::commands::realm_members::{KeyArgs, Member};
use actr_hyper::{ActrRef, Node, WorkloadPackage};
use actr_protocol::{ActrId, PayloadType, RpcRequest};
use prost::Message;

mod documents {
    #![allow(dead_code)]
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/e2e/actr/package-runtime-echo/fixtures/documents.rs"
    ));
}

#[derive(Message)]
struct EchoRequest {
    #[prost(string, tag = "1")]
    message: String,
}

#[derive(Message)]
struct EchoResponse {
    #[prost(string, tag = "1")]
    reply: String,
    #[prost(uint64, tag = "2")]
    timestamp: u64,
}

impl RpcRequest for EchoRequest {
    type Response = EchoResponse;

    fn route_key() -> &'static str {
        "echo.EchoService.Echo"
    }

    fn payload_type() -> PayloadType {
        PayloadType::RpcReliable
    }
}

#[derive(Deserialize)]
struct Request {
    owner_key: PathBuf,
    front_key: PathBuf,
    selection: PathBuf,
    output: PathBuf,
    public_profile_url: String,
    issuer_profile_url: String,
    issuer_ca: PathBuf,
    package_key: String,
    service_actors: Option<[String; 2]>,
}

/// Exercise the release CLI and native Front before publication and after
/// both release and rollback against the live Service Actors.
#[tokio::test]
#[ignore = "started by package-runtime-echo/lib/realm-scenarios.sh"]
async fn realm_front_release_fixture() -> Result<()> {
    let request: Request =
        serde_json::from_slice(&std::fs::read(std::env::var("ACTR_REALM_E2E_REQUEST")?)?)?;
    let owner = Member::load(&KeyArgs {
        key: request.owner_key,
    })?;
    let realm = &owner.file.realm;
    let provider = &owner.file.provider_id;
    let direct = reqwest::Client::builder()
        .https_only(true)
        .tls_built_in_root_certs(false)
        .add_root_certificate(reqwest::Certificate::from_pem(&std::fs::read(
            request.issuer_ca,
        )?)?)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()?;
    let get = |url: String, limit| {
        let direct = &direct;
        let public_url = &request.public_profile_url;
        let issuer_url = &request.issuer_profile_url;
        async move {
            if &url == public_url {
                let bytes = direct
                    .get(issuer_url)
                    .send()
                    .await?
                    .error_for_status()?
                    .bytes()
                    .await?;
                ensure!(bytes.len() <= limit, "Profile body exceeds limit");
                Ok(bytes.to_vec())
            } else {
                documents::fetch_https(&url, limit).await
            }
        }
    };
    let args = FetchArgs {
        realm: realm.clone(),
        additional: vec![],
        output: request.output.clone(),
    };
    let mut previous = None;
    let mut original = None;
    let mut original_selection: Option<RealmPackageSelection> = None;
    for step in 0..3 {
        if step > 0 {
            let selection = (step == 1)
                .then(|| crate::commands::realm::read_selection(&request.selection))
                .transpose()?;
            if request.service_actors.is_some() {
                let (baseline, _, _) =
                    resolve_profile(realm, None, |url| get(url, DOCUMENT_LIMIT), None).await?;
                let target_selection = selection
                    .as_ref()
                    .or(original_selection.as_ref())
                    .context("previous selection")?;
                let (target, _) = target_selection.selected().map_err(anyhow::Error::msg)?;
                let probe_dir = request.output.join(format!("probe-{step}"));
                let probe_config = front_config(
                    &baseline,
                    target,
                    &request.front_key,
                    &request.package_key,
                    &probe_dir,
                )?;
                let payload = probe_dir.join("echo.pb");
                std::fs::write(
                    &payload,
                    EchoRequest {
                        message: format!("ready-{step}"),
                    }
                    .encode_to_vec(),
                )?;
                let mut probe = front::ServiceProbe {
                    probe_config,
                    probe_request: Some(payload),
                    probe_route: EchoRequest::route_key().to_owned(),
                };
                if step == 1 {
                    probe.probe_route = "echo.EchoService.NotReady".into();
                    let refused = front::release_selection_with(
                        realm,
                        provider,
                        &request.output,
                        selection.clone(),
                        Some(&probe),
                        &get,
                        |_| async { bail!("unready Service attempted publication") },
                    )
                    .await
                    .unwrap_err();
                    ensure!(
                        refused
                            .to_string()
                            .contains("did not pass the readiness RPC"),
                        "unexpected probe failure: {refused:#}"
                    );
                    let (unchanged, _, _) =
                        resolve_profile(realm, None, |url| get(url, DOCUMENT_LIMIT), None).await?;
                    ensure!(
                        unchanged.sequence == baseline.sequence,
                        "failed probe changed the Profile"
                    );
                    println!(
                        "SERVICE_READINESS_GATE sequence={} rejected_unready={}",
                        baseline.sequence, target.r#type
                    );
                    probe.probe_route = EchoRequest::route_key().to_owned();
                }
                front::release_selection_with(
                    realm,
                    provider,
                    &request.output,
                    selection,
                    Some(&probe),
                    &get,
                    |operation| owner.send(operation),
                )
                .await?;
            } else {
                front::release_with(
                    realm,
                    provider,
                    &request.output,
                    selection,
                    &get,
                    |operation| owner.send(operation),
                )
                .await?;
            }
        }
        let pair = fetch_with(&args, &get).await?;
        let (profile, _, _) =
            resolve_profile(realm, None, |url| get(url, DOCUMENT_LIMIT), None).await?;
        let selection = profile.selection.as_ref().context("active selection")?;
        let (service, front) = selection.selected().map_err(anyhow::Error::msg)?;
        let front_archive = std::fs::read(pair["front"].as_str().context("verified Front path")?)?;
        let (candidate, service_actor) = call_echo(
            &profile,
            front_archive,
            &request.front_key,
            &request.package_key,
            &request.output.join(format!("runtime-{step}")),
        )
        .await?;
        if let Some(actors) = &request.service_actors {
            let expected = &actors[usize::from(step == 1)];
            ensure!(
                service_actor.to_string_repr() == *expected,
                "RPC reached the wrong Service Actor"
            );
            let document: Value = serde_json::from_slice(profile.document())?;
            ensure!(
                document.get("service_actor_id").is_none(),
                "signed Profile pinned an ActorId"
            );
        }
        println!(
            "{} sequence={} front={} front_sha256={} service={} service_sha256={} service_actor={}",
            if request.service_actors.is_some() {
                "SERVICE_SWITCH"
            } else {
                "FRONT_RELEASE"
            },
            profile.sequence,
            front.r#type,
            front.sha256,
            service.r#type,
            service.sha256,
            service_actor.to_string_repr()
        );
        if let Some((first_sequence, first_service, first_front, first_actor)) = &original {
            ensure!(
                profile.sequence == first_sequence + step,
                "unexpected Profile sequence"
            );
            if request.service_actors.is_none() || step == 2 {
                ensure!(
                    service == first_service && service_actor == *first_actor,
                    "did not call the original Service"
                );
            } else {
                ensure!(
                    service.r#type != first_service.r#type
                        && service.sha256 != first_service.sha256
                        && service_actor != *first_actor,
                    "candidate Service was not activated"
                );
            }
            if step == 1 {
                ensure!(front != first_front, "Front did not upgrade");
            } else {
                ensure!(
                    front == first_front,
                    "rollback did not restore the original Front"
                );
            }
        } else {
            original_selection = Some(selection.clone());
            original = Some((
                profile.sequence,
                service.clone(),
                front.clone(),
                service_actor,
            ));
        }
        if let Some(old) = previous.replace(candidate) {
            old.shutdown();
            old.wait_for_shutdown().await;
        }
    }
    if let Some(last) = previous {
        last.shutdown();
        last.wait_for_shutdown().await;
    }
    Ok(())
}

async fn call_echo(
    profile: &RealmProfile,
    archive: Vec<u8>,
    front_key: &Path,
    package_key: &str,
    directory: &Path,
) -> Result<(ActrRef, ActrId)> {
    let (service, _) = profile
        .selection
        .as_ref()
        .context("active selection")?
        .selected()
        .map_err(anyhow::Error::msg)?;
    let target = service.actr_type().map_err(anyhow::Error::msg)?;
    let config = front_config(profile, service, front_key, package_key, directory)?;
    let candidate = Node::from_config_file(&config)
        .await?
        .attach_front(&WorkloadPackage::new(archive), &target)
        .await?
        .register(&profile.issuer.url)
        .await?
        .start()
        .await?;
    let actor = candidate.discover_service(&target).await?;
    let message = format!("package-release-{}", profile.sequence);
    let response = candidate
        .call_remote(
            actor.clone(),
            EchoRequest {
                message: message.clone(),
            },
            30_000,
        )
        .await?;
    ensure!(
        response.reply.ends_with(&message),
        "Echo RPC did not return the request"
    );
    Ok((candidate, actor))
}

fn front_config(
    profile: &RealmProfile,
    service: &RealmPackage,
    front_key: &Path,
    package_key: &str,
    directory: &Path,
) -> Result<PathBuf> {
    let signaling = profile
        .provider
        .signaling
        .iter()
        .min_by_key(|endpoint| endpoint.priority)
        .context("no signaling endpoint")?;
    let quote = |value: &str| serde_json::to_string(value).expect("JSON string");
    let ice_urls = |kind: &str| {
        let insecure = format!("{kind}:");
        let secure = format!("{kind}s:");
        profile
            .provider
            .ice
            .iter()
            .flat_map(|endpoint| &endpoint.urls)
            .filter(|url| url.starts_with(&insecure) || url.starts_with(&secure))
            .collect::<Vec<_>>()
    };
    std::fs::create_dir_all(directory)?;
    let config = directory.join("actr.toml");
    std::fs::write(
        &config,
        format!(
            "edition = 1\n[signaling]\nurl = {}\n[issuer_endpoint]\nurl = {}\n[deployment]\nrealm = {}\nrealm_client_key = {}\n[hyper]\ndata_dir = {}\n[discovery]\nvisible = false\n[webrtc]\nforce_relay = true\nstun_urls = {}\nturn_urls = {}\n[[acl.rules]]\npermission = \"allow\"\ntype = {}\nrealm = \"self\"\n[[trust]]\nkind = \"static\"\npubkey_b64 = {}\n",
            quote(&signaling.url),
            quote(&profile.issuer.url),
            quote(&profile.realm.name),
            quote(front_key.to_str().context("non-UTF-8 Front key path")?),
            quote(directory.to_str().context("non-UTF-8 runtime directory")?),
            serde_json::to_string(&ice_urls("stun"))?,
            serde_json::to_string(&ice_urls("turn"))?,
            quote(&service.r#type),
            quote(package_key),
        ),
    )?;
    Ok(config)
}
