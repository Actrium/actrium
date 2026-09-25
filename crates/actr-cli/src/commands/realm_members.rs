//! `actr realm join | invite | members | …` — a member's tool
//! (`actr/protocols/realm-membership.md`).
//!
//! `join` redeems a Realm invite with a new Realm client key and writes the
//! key file; the other commands sign a membership request with that key and
//! send it to `<issuer>/membership`. Replies are printed as JSON.

use actr_protocol::realm_membership::{
    MembershipOp, MembershipRequest, RealmClientKeyFile, RealmInvite, Role,
};
use anyhow::{Context, Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use clap::Args;
use ed25519_dalek::SigningKey;
use serde_json::Value;
use std::path::PathBuf;

use super::realm::write_private;

fn parse_role(text: &str) -> std::result::Result<Role, String> {
    match Role::parse(text) {
        Some(Role::Owner) | None => Err("expected editor or viewer".to_string()),
        Some(role) => Ok(role),
    }
}

#[derive(Args, Debug, Clone)]
pub struct JoinArgs {
    /// Realm invite text `ri_…` (`-` reads stdin)
    #[arg(value_name = "INVITE")]
    pub invite: String,
    /// Key file to write the new Realm client key to
    #[arg(long, short = 'o', value_name = "FILE")]
    pub output: PathBuf,
    /// Issuer URL of the Realm (`issuer.url` of its Realm Profile). An
    /// invite's embedded Realm Profile is not verified here, so it is not
    /// used.
    #[arg(long, value_name = "URL")]
    pub issuer: String,
    /// Realm Provider ID; defaults to the Issuer URL's `host[:port]`
    #[arg(long, value_name = "ID")]
    pub provider: Option<String>,
    /// Name of this client, shown to the Realm owner
    #[arg(long, value_name = "TEXT")]
    pub label: Option<String>,
    /// Overwrite an existing key file
    #[arg(long)]
    pub force: bool,
}

#[derive(Args, Debug)]
pub struct KeyArgs {
    /// Realm client key file from `actr realm join`
    #[arg(long, value_name = "FILE")]
    pub key: PathBuf,
}

#[derive(Args, Debug)]
pub struct InviteArgs {
    #[command(flatten)]
    pub key: KeyArgs,
    /// Invite a new member with this role (owner only)
    #[arg(long, value_name = "ROLE", value_parser = parse_role, conflicts_with = "member", required_unless_present = "member")]
    pub role: Option<Role>,
    /// Invite a new client of this existing member
    #[arg(long, value_name = "MEMBER_ID")]
    pub member: Option<String>,
    /// Days the invite stays valid (at most 30)
    #[arg(long, value_name = "DAYS", default_value_t = 7)]
    pub days: u32,
}

#[derive(Args, Debug)]
pub struct InviteIdArgs {
    #[command(flatten)]
    pub key: KeyArgs,
    #[arg(value_name = "INVITE_ID")]
    pub invite_id: String,
}

#[derive(Args, Debug)]
pub struct RevokeClientArgs {
    #[command(flatten)]
    pub key: KeyArgs,
    /// Client to revoke; this key file's own key if absent
    #[arg(value_name = "KEY_ID")]
    pub key_id: Option<String>,
}

#[derive(Args, Debug)]
pub struct MemberArgs {
    #[command(flatten)]
    pub key: KeyArgs,
    #[arg(value_name = "MEMBER_ID")]
    pub member_id: String,
}

#[derive(Args, Debug)]
pub struct SetRoleArgs {
    #[command(flatten)]
    pub key: KeyArgs,
    #[arg(value_name = "MEMBER_ID")]
    pub member_id: String,
    #[arg(value_name = "ROLE", value_parser = parse_role)]
    pub role: Role,
}

#[derive(Args, Debug)]
pub struct TransferOwnerArgs {
    #[command(flatten)]
    pub key: KeyArgs,
    #[arg(value_name = "MEMBER_ID")]
    pub member_id: String,
    /// Your role after the transfer
    #[arg(long, value_name = "ROLE", value_parser = parse_role, default_value = "editor")]
    pub previous_owner_role: Role,
}

/// A loaded key file.
struct Member {
    file: RealmClientKeyFile,
    key: SigningKey,
}

impl Member {
    fn load(args: &KeyArgs) -> Result<Self> {
        let text = std::fs::read_to_string(&args.key)
            .with_context(|| format!("Failed to read key file: {}", args.key.display()))?;
        let (file, key) = RealmClientKeyFile::parse(&text)
            .with_context(|| format!("Not a Realm client key file: {}", args.key.display()))?;
        Ok(Self { file, key })
    }

    fn request(&self, op: MembershipOp) -> MembershipRequest {
        MembershipRequest::new(op, &self.file.realm, &self.file.provider_id, &new_nonce())
    }

    async fn send(&self, request: MembershipRequest) -> Result<Value> {
        Ok(send(&self.file.issuer, &request.sign(&self.key)).await?)
    }
}

fn new_nonce() -> String {
    hex::encode(rand::random::<[u8; 16]>())
}

/// `host[:port]` of `url`, lowercase: the Realm Provider ID a Realm without
/// a Realm Profile is addressed by.
fn url_authority(url: &str) -> Result<String> {
    let parsed = url::Url::parse(url).with_context(|| format!("Invalid Issuer URL: {url}"))?;
    let host = parsed
        .host_str()
        .with_context(|| format!("Issuer URL has no host: {url}"))?;
    Ok(match parsed.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    }
    .to_ascii_lowercase())
}

/// Why a membership request got no reply.
enum SendError {
    /// The Realm Provider answered with an error: nothing changed.
    Refused(anyhow::Error),
    /// No usable answer: the request may or may not have taken effect.
    Unknown(anyhow::Error),
}

impl From<SendError> for anyhow::Error {
    fn from(error: SendError) -> Self {
        match error {
            SendError::Refused(error) | SendError::Unknown(error) => error,
        }
    }
}

/// POST `document` to `<issuer>/membership`; the reply, or its error.
async fn send(issuer: &str, document: &Value) -> std::result::Result<Value, SendError> {
    let url = format!("{}/membership", issuer.trim_end_matches('/'));
    let response = reqwest::Client::new()
        .post(&url)
        .header("Content-Type", "application/json")
        .body(document.to_string())
        .send()
        .await
        .with_context(|| format!("Failed to send the membership request to {url}"))
        .map_err(SendError::Unknown)?;
    let status = response.status();
    let body = response
        .text()
        .await
        .with_context(|| format!("Failed to read the membership reply from {url}"))
        .map_err(SendError::Unknown)?;
    if !status.is_success() {
        return Err(SendError::Refused(anyhow::anyhow!(
            "membership request refused (HTTP {status}): {body}"
        )));
    }
    serde_json::from_str(&body)
        .context("membership reply is not JSON")
        .map_err(SendError::Unknown)
}

/// The key of a key file for `realm` at `issuer` that an interrupted `join`
/// left at `path`, if any.
fn interrupted_join_key(path: &std::path::Path, realm: &str, issuer: &str) -> Option<SigningKey> {
    let text = std::fs::read_to_string(path).ok()?;
    let (file, key) = RealmClientKeyFile::parse(&text).ok()?;
    (file.realm == realm && file.issuer == issuer).then_some(key)
}

/// Redeem the invite. The key file is written before the request is sent,
/// so a lost reply does not lose the key of a redeemed invite: rerunning the
/// same command reuses the key file, and the Realm Provider answers the
/// retry of that redemption with its result (or `conflict` if the key is
/// already registered otherwise).
pub async fn join(args: &JoinArgs) -> Result<Value> {
    let text = if args.invite == "-" {
        std::io::read_to_string(std::io::stdin())?
    } else {
        args.invite.clone()
    };
    let invite = RealmInvite::decode(&text)?;
    // The invite's embedded Realm Profile is unverified: it must not choose
    // where the invite secret goes.
    let issuer = args.issuer.clone();
    let provider = match &args.provider {
        Some(provider) => provider.clone(),
        None => url_authority(&issuer)?,
    };

    let reused = if args.output.exists() && !args.force {
        let key = interrupted_join_key(&args.output, &invite.realm, &issuer);
        if key.is_none() {
            bail!(
                "Key file already exists: {}\nUse --force to overwrite.",
                args.output.display()
            );
        }
        key
    } else {
        None
    };
    let created = reused.is_none();
    let key = match reused {
        Some(key) => key,
        None => {
            let key = SigningKey::from_bytes(&rand::random::<[u8; 32]>());
            if let Some(parent) = args.output.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create {}", parent.display()))?;
            }
            let file = RealmClientKeyFile::new(&invite.realm, &provider, &issuer, &key);
            write_private(
                &args.output,
                serde_json::to_string_pretty(&file)?.as_bytes(),
            )
            .with_context(|| format!("Failed to write key file: {}", args.output.display()))?;
            key
        }
    };

    let mut request =
        MembershipRequest::new(MembershipOp::Join, &invite.realm, &provider, &new_nonce());
    request.invite = Some(invite.secret.clone());
    request.public_key = Some(BASE64.encode(key.verifying_key().to_bytes()));
    request.label = args.label.clone();
    match send(&issuer, &request.sign(&key)).await {
        Ok(reply) => {
            eprintln!("Realm client key written to {}", args.output.display());
            Ok(reply)
        }
        Err(SendError::Refused(error)) => {
            if created {
                let _ = std::fs::remove_file(&args.output);
            }
            Err(error)
        }
        Err(SendError::Unknown(error)) => Err(error.context(format!(
            "The invite may have been redeemed; the key is kept in {}. \
             Rerun the same command to finish joining.",
            args.output.display()
        ))),
    }
}

pub async fn invite(args: &InviteArgs) -> Result<Value> {
    if args.days == 0 || args.days > 30 {
        bail!("--days must be 1 to 30");
    }
    let member = Member::load(&args.key)?;
    let mut request = member.request(MembershipOp::CreateInvite);
    request.role = args.role;
    request.member_id = args.member.clone();
    let expires = chrono::Utc::now() + chrono::Duration::days(args.days.into());
    request.invite_expires_at = Some(expires.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    member.send(request).await
}

pub async fn members(args: &KeyArgs) -> Result<Value> {
    let member = Member::load(args)?;
    member.send(member.request(MembershipOp::ListMembers)).await
}

pub async fn revoke_invite(args: &InviteIdArgs) -> Result<Value> {
    let member = Member::load(&args.key)?;
    let mut request = member.request(MembershipOp::RevokeInvite);
    request.invite_id = Some(args.invite_id.clone());
    member.send(request).await
}

pub async fn revoke_client(args: &RevokeClientArgs) -> Result<Value> {
    let member = Member::load(&args.key)?;
    let mut request = member.request(MembershipOp::RevokeClient);
    request.key_id = Some(args.key_id.clone().unwrap_or(member.file.key_id.clone()));
    member.send(request).await
}

pub async fn remove_member(args: &MemberArgs) -> Result<Value> {
    let member = Member::load(&args.key)?;
    let mut request = member.request(MembershipOp::RemoveMember);
    request.member_id = Some(args.member_id.clone());
    member.send(request).await
}

pub async fn set_role(args: &SetRoleArgs) -> Result<Value> {
    let member = Member::load(&args.key)?;
    let mut request = member.request(MembershipOp::SetRole);
    request.member_id = Some(args.member_id.clone());
    request.role = Some(args.role);
    member.send(request).await
}

pub async fn transfer_owner(args: &TransferOwnerArgs) -> Result<Value> {
    let member = Member::load(&args.key)?;
    let mut request = member.request(MembershipOp::TransferOwner);
    request.member_id = Some(args.member_id.clone());
    request.previous_owner_role = Some(args.previous_owner_role);
    member.send(request).await
}

pub async fn leave(args: &KeyArgs) -> Result<Value> {
    let member = Member::load(args)?;
    member.send(member.request(MembershipOp::Leave)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Json, Router, routing::post};
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    const REALM: &str = "k7m2q9xh@acme.com";

    /// A membership endpoint that verifies `join` with its own key and any
    /// other request with the key that joined, and echoes the request.
    async fn serve() -> String {
        let joined = std::sync::Arc::new(std::sync::Mutex::new(None::<[u8; 32]>));
        let app = Router::new().route(
            "/issuer/membership",
            post(move |body: axum::body::Bytes| {
                let joined = joined.clone();
                async move {
                    let request = MembershipRequest::parse(&body).unwrap();
                    if request.op == MembershipOp::Join {
                        *joined.lock().unwrap() = Some(request.verify_join().unwrap());
                    } else {
                        request.verify(&joined.lock().unwrap().unwrap()).unwrap();
                    }
                    Json(serde_json::from_slice::<Value>(&body).unwrap())
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}/issuer")
    }

    #[tokio::test]
    async fn join_writes_a_key_file_the_other_commands_sign_with() {
        let issuer = serve().await;
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("keys/client.json");
        let secret = URL_SAFE_NO_PAD.encode([1u8; 32]);
        let join_args = JoinArgs {
            invite: RealmInvite::new(REALM, &secret).encode(),
            output: output.clone(),
            issuer: issuer.clone(),
            provider: None,
            label: Some("laptop".into()),
            force: false,
        };
        let sent = join(&join_args).await.unwrap();
        assert_eq!(sent["op"], "join");
        assert_eq!(sent["invite"], secret);
        assert_eq!(sent["label"], "laptop");
        // No Realm Profile: the provider is the Issuer URL's authority.
        assert_eq!(sent["provider_id"], url_authority(&issuer).unwrap());
        let key = KeyArgs {
            key: output.clone(),
        };
        assert_eq!(Member::load(&key).unwrap().file.realm, REALM);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(&output).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        // A rerun reuses the key file (retry of an interrupted join); a key
        // file of another Realm is not overwritten by accident.
        let file = Member::load(&key).unwrap().file;
        assert_eq!(
            join(&join_args).await.unwrap()["public_key"],
            file.public_key
        );
        assert!(
            join(&JoinArgs {
                invite: RealmInvite::new("other@acme.com", &secret).encode(),
                ..join_args.clone()
            })
            .await
            .is_err()
        );
        assert_eq!(Member::load(&key).unwrap().file.public_key, file.public_key);

        let sent = invite(&InviteArgs {
            key: KeyArgs {
                key: key.key.clone(),
            },
            role: Some(Role::Editor),
            member: None,
            days: 7,
        })
        .await
        .unwrap();
        assert_eq!(
            (sent["op"].as_str(), sent["role"].as_str()),
            (Some("create_invite"), Some("editor"))
        );

        let sent = revoke_client(&RevokeClientArgs {
            key: KeyArgs {
                key: key.key.clone(),
            },
            key_id: None,
        })
        .await
        .unwrap();
        let file = Member::load(&key).unwrap().file;
        assert_eq!(sent["key_id"], file.key_id.as_str());

        let sent = transfer_owner(&TransferOwnerArgs {
            key: KeyArgs {
                key: key.key.clone(),
            },
            member_id: "0123456789abcdef".into(),
            previous_owner_role: Role::Viewer,
        })
        .await
        .unwrap();
        assert_eq!(sent["previous_owner_role"], "viewer");
        assert_eq!(leave(&key).await.unwrap()["op"], "leave");
    }

    /// A membership endpoint that reads each request, sends it on the
    /// returned channel and closes the connection without a reply.
    async fn serve_without_reply() -> (String, tokio::sync::mpsc::UnboundedReceiver<Value>) {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buffer = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let read = stream.read(&mut chunk).await.unwrap();
                    if read == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..read]);
                    let text = String::from_utf8_lossy(&buffer);
                    if let Some(start) = text.find("\r\n\r\n")
                        && let Ok(body) = serde_json::from_str::<Value>(&text[start + 4..])
                    {
                        let _ = tx.send(body);
                        break;
                    }
                }
                let _ = stream.shutdown().await;
            }
        });
        (format!("http://{address}/issuer"), rx)
    }

    #[tokio::test]
    async fn a_join_whose_reply_is_lost_keeps_its_key_for_the_retry() {
        let (lost, mut sent) = serve_without_reply().await;
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("client.json");
        let args = JoinArgs {
            invite: RealmInvite::new(REALM, &URL_SAFE_NO_PAD.encode([3u8; 32])).encode(),
            output: output.clone(),
            issuer: lost,
            provider: Some("p.example".into()),
            label: None,
            force: false,
        };
        let error = join(&args).await.unwrap_err();
        assert!(format!("{error:#}").contains("Rerun the same command"));
        let first = sent.recv().await.unwrap();
        // The key the Realm Provider may have registered is on disk.
        let file = Member::load(&KeyArgs {
            key: output.clone(),
        })
        .unwrap()
        .file;
        assert_eq!(first["public_key"], file.public_key);

        // Rerunning the command retries the same redemption with that key.
        assert!(join(&args).await.is_err());
        let retry = sent.recv().await.unwrap();
        assert_eq!(retry["public_key"], file.public_key);
        assert_eq!(retry["invite"], first["invite"]);
    }

    #[tokio::test]
    async fn a_refused_join_leaves_no_key_file() {
        let app = Router::new().route(
            "/issuer/membership",
            post(|| async {
                (
                    axum::http::StatusCode::GONE,
                    Json(serde_json::json!({"error": "invite_used", "message": "used"})),
                )
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("client.json");
        let error = join(&JoinArgs {
            invite: RealmInvite::new(REALM, &URL_SAFE_NO_PAD.encode([4u8; 32])).encode(),
            output: output.clone(),
            issuer: format!("http://{address}/issuer"),
            provider: None,
            label: None,
            force: false,
        })
        .await
        .unwrap_err();
        assert!(error.to_string().contains("invite_used"));
        assert!(!output.exists());
    }

    #[tokio::test]
    async fn join_does_not_trust_an_embedded_realm_profile() {
        let issuer = serve().await;
        let dir = tempfile::tempdir().unwrap();
        let mut invite = RealmInvite::new(REALM, &URL_SAFE_NO_PAD.encode([2u8; 32]));
        // Unverified: its issuer and provider must not be used.
        invite.profile = Some(serde_json::json!({
            "realm": { "name": REALM },
            "provider": { "id": "evil.example" },
            "issuer": { "url": "http://127.0.0.1:9/issuer" },
        }));
        let sent = join(&JoinArgs {
            invite: invite.encode(),
            output: dir.path().join("client.json"),
            issuer: issuer.clone(),
            provider: None,
            label: None,
            force: false,
        })
        .await
        .unwrap();
        assert_eq!(sent["provider_id"], url_authority(&issuer).unwrap());
    }

    #[test]
    fn roles_exclude_owner() {
        assert!(parse_role("owner").is_err());
        assert_eq!(parse_role("viewer").unwrap(), Role::Viewer);
        assert_eq!(
            url_authority("HTTP://Example.COM:8081/issuer").unwrap(),
            "example.com:8081"
        );
        assert_eq!(
            url_authority("https://example.com/issuer").unwrap(),
            "example.com"
        );
    }
}
