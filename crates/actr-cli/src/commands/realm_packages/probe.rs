//! Check a deployed Service through a verified Front and the selected RPC.

use super::*;

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
        })
    }
}

pub struct ServiceProbe {
    pub probe_config: PathBuf,
    pub probe_route: String,
    pub probe_request: Option<PathBuf>,
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
        let mut payload = Vec::new();
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
