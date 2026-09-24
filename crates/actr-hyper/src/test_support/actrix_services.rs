//! In-process Actrix services used by RTC integration and E2E harnesses.
//!
//! This fixture deliberately composes the production Actrix Signaling, STUN,
//! and TURN crates. It is not a protocol mock: only process lifecycle and
//! deterministic test credentials live here.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use actr_protocol::prost::Message as _;
use actr_protocol::{AIdCredential, ActrId, IdentityClaims, TurnCredential};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use ed25519_dalek::{Signer as _, SigningKey};
use hmac::{Hmac, Mac as _};
use sha1::Sha1;
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::{OnceCell, broadcast};
use tokio_util::sync::CancellationToken;

use super::{TEST_REALM, make_actor_type};
use webrtc::peer_connection::{
    RTCConfiguration, RTCConfigurationBuilder, RTCIceServer, RTCIceTransportPolicy,
};

const TURN_REALM: &str = "actr.test";
const TURN_SHARED_TEST_KEY: &str = "actrix-rtc-integration-test-secret";
const CREDENTIAL_LIFETIME: Duration = Duration::from_secs(10 * 60);
const AID_KEY_ID: u32 = 7;
const AID_SIGNING_KEY_BYTES: [u8; 32] = [0x5a; 32];

type HmacSha1 = Hmac<Sha1>;

static SIGNALING_DATABASE_INIT: OnceCell<PathBuf> = OnceCell::const_new();

fn aid_signing_key() -> SigningKey {
    SigningKey::from_bytes(&AID_SIGNING_KEY_BYTES)
}

async fn initialize_signaling_database() -> anyhow::Result<PathBuf> {
    let sqlite_path = SIGNALING_DATABASE_INIT
        .get_or_try_init(|| async {
            if !platform::storage::db::is_database_initialized() {
                let db_dir = std::env::temp_dir().join(format!(
                    "actrix_rtc_test_db_{}_{}",
                    std::process::id(),
                    uuid::Uuid::new_v4()
                ));
                std::fs::create_dir_all(&db_dir)?;
                platform::storage::db::set_db_path(&db_dir).await?;
            }

            let sqlite_path = std::env::temp_dir().join(format!(
                "actrix_rtc_signaling_{}_{}",
                std::process::id(),
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir_all(&sqlite_path)?;

            // `make_actor_id` uses `TEST_REALM`. The production signaling
            // router validates every websocket identity against the Actrix
            // realm store before registering it; an empty schema is therefore
            // not a usable signaling fixture.
            platform::Realm::upsert_managed(
                TEST_REALM.to_string(),
                true,
                None,
                platform::realm::hash_realm_secret("actr-rtc-integration-secret"),
                None,
                None,
            )
            .await?;

            let actor_type = make_actor_type().to_string_repr();
            if platform::ActorAcl::get_by_types(TEST_REALM, TEST_REALM, &actor_type, &actor_type)
                .await?
                .is_none()
            {
                let mut acl = platform::ActorAcl::new(
                    TEST_REALM.to_string(),
                    actor_type.clone(),
                    actor_type,
                    true,
                );
                acl.save().await?;
            }

            platform::AIdCredentialValidator::init(&sqlite_path).await?;
            let key_expires_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock before Unix epoch")
                .as_secs()
                + 24 * 60 * 60;
            platform::AIdCredentialValidator::populate_key(
                AID_KEY_ID,
                &aid_signing_key().verifying_key(),
                key_expires_at,
            )
            .await?;

            Ok::<PathBuf, anyhow::Error>(sqlite_path)
        })
        .await?;
    Ok(sqlite_path.clone())
}

/// Real Actrix Signaling/STUN/TURN endpoints with bounded shutdown handles.
pub struct ActrixTestServices {
    signaling_addr: SocketAddr,
    stun_addr: SocketAddr,
    turn_addr: SocketAddr,
    signaling_cancel: CancellationToken,
    signaling_task: tokio::task::JoinHandle<()>,
    stun_shutdown: broadcast::Sender<()>,
    stun_task: tokio::task::JoinHandle<()>,
    turn_server: Option<turn_crate::server::Server>,
}

impl ActrixTestServices {
    /// Start all three production Actrix protocol implementations on loopback
    /// with ephemeral ports.
    pub async fn start() -> anyhow::Result<Self> {
        let sqlite_path = initialize_signaling_database().await?;

        let signaling_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let signaling_addr = signaling_listener.local_addr()?;
        let signaling_cancel = CancellationToken::new();
        let signaling_config = platform::config::ActrixConfig {
            env: "test".to_string(),
            sqlite_path,
            ..Default::default()
        };
        let signaling_router = actrix_signaling_service::create_signaling_router_with_config(
            &signaling_config,
            signaling_cancel.clone(),
        )
        .await?;
        let signaling_app = axum::Router::new().nest("/signaling", signaling_router);
        let cancel = signaling_cancel.clone();
        let signaling_task = tokio::spawn(async move {
            let result = axum::serve(
                signaling_listener,
                signaling_app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(cancel.cancelled_owned())
            .await;
            if let Err(error) = result {
                tracing::error!(%error, "Actrix signaling test service stopped unexpectedly");
            }
        });

        let stun_socket = Arc::new(UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await?);
        let stun_addr = stun_socket.local_addr()?;
        let (stun_shutdown, stun_shutdown_rx) = broadcast::channel(1);
        let stun_task = tokio::spawn(async move {
            if let Err(error) =
                actrix_stun_service::create_stun_server_with_shutdown(stun_socket, stun_shutdown_rx)
                    .await
            {
                tracing::error!(%error, "Actrix STUN test service stopped unexpectedly");
            }
        });

        let turn_socket = Arc::new(UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await?);
        let turn_addr = turn_socket.local_addr()?;
        let authenticator = Arc::new(
            turn::Authenticator::new(TURN_SHARED_TEST_KEY.to_owned())
                .map_err(|error| anyhow::anyhow!("Actrix TURN authenticator: {error}"))?,
        );
        let turn_server = turn::create_turn_server(
            turn_socket,
            &IpAddr::V4(Ipv4Addr::LOCALHOST).to_string(),
            TURN_REALM,
            (49_152, 65_535),
            authenticator,
            true,
        )
        .await
        .map_err(|error| anyhow::anyhow!("Actrix TURN service: {error}"))?;

        Ok(Self {
            signaling_addr,
            stun_addr,
            turn_addr,
            signaling_cancel,
            signaling_task,
            stun_shutdown,
            stun_task,
            turn_server: Some(turn_server),
        })
    }

    /// Production Actrix Signaling WebSocket endpoint.
    pub fn signaling_url(&self) -> String {
        format!("ws://{}/signaling/ws", self.signaling_addr)
    }

    pub fn signaling_addr(&self) -> SocketAddr {
        self.signaling_addr
    }

    pub fn stun_addr(&self) -> SocketAddr {
        self.stun_addr
    }

    pub fn turn_addr(&self) -> SocketAddr {
        self.turn_addr
    }

    /// Generate the same time-limited HMAC credential format Actrix Issuer
    /// issues to the production TURN authenticator.
    pub fn turn_credential(&self, actor_id: &ActrId) -> TurnCredential {
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .saturating_add(CREDENTIAL_LIFETIME)
            .as_secs();
        // Username must match the scoped format enforced by `turn::Authenticator`:
        // `{expires_at}:realm={realm}:provider=actrix-official:audience=actr.turn:actor={actr_id}`
        let username = format!(
            "{expires_at}:realm={}:provider=actrix-official:audience=actr.turn:actor={actor_id}",
            actor_id.realm
        );
        let mut mac = HmacSha1::new_from_slice(TURN_SHARED_TEST_KEY.as_bytes())
            .expect("HMAC accepts an arbitrary key length");
        mac.update(username.as_bytes());
        TurnCredential {
            username,
            password: BASE64_STANDARD.encode(mac.finalize().into_bytes()),
            expires_at,
            protocol_version: 1,
            realm: actor_id.realm.clone(),
            provider: "actrix-official".to_string(),
            audience: "actr.turn".to_string(),
        }
    }

    /// Produce a short-lived AId credential signed exactly as Issuer credentials
    /// are signed. The corresponding public key is installed in Actrix's
    /// production validator during fixture startup.
    pub fn aid_credential(&self, actor_id: &ActrId) -> AIdCredential {
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .saturating_add(CREDENTIAL_LIFETIME)
            .as_secs();
        let claims = IdentityClaims {
            realm: actor_id.realm.clone(),
            actor_id: actor_id.to_string_repr(),
            expires_at,
            protocol_version: 1,
            issuer: actor_id.realm.clone(),
            provider: "actrix-official".to_string(),
            audience: "actr.signaling".to_string(),
            actor_type: make_actor_type(),
        }
        .encode_to_vec();
        let signature = aid_signing_key().sign(&claims).to_bytes();
        AIdCredential {
            key_id: AID_KEY_ID,
            claims: claims.into(),
            signature: signature.to_vec().into(),
        }
    }

    /// Complete ICE configuration backed only by services started by this
    /// Actrix fixture. `relay_only` is used by T4 relay-path acceptance cases.
    pub fn rtc_configuration(&self, actor_id: &ActrId, relay_only: bool) -> RTCConfiguration {
        let credential = self.turn_credential(actor_id);
        let servers = vec![
            RTCIceServer {
                urls: vec![format!("stun:{}", self.stun_addr)],
                ..Default::default()
            },
            RTCIceServer {
                urls: vec![format!("turn:{}?transport=udp", self.turn_addr)],
                username: credential.username,
                credential: credential.password,
            },
        ];
        let builder = RTCConfigurationBuilder::default().with_ice_servers(servers);
        if relay_only {
            builder
                .with_ice_transport_policy(RTCIceTransportPolicy::Relay)
                .build()
        } else {
            builder.build()
        }
    }

    /// Stop all services and wait for their owning tasks to exit.
    pub async fn shutdown(mut self) {
        self.signaling_cancel.cancel();
        let _ = self.stun_shutdown.send(());
        if let Some(server) = self.turn_server.take()
            && let Err(error) = turn::shutdown_turn_server(&server).await
        {
            tracing::warn!(%error, "Actrix TURN test shutdown failed");
        }
        let _ = self.signaling_task.await;
        let _ = self.stun_task.await;
    }
}
