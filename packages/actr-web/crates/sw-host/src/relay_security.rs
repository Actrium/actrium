//! Worker-only end-to-end security. Enrollment proofs and private keys never
//! cross the DOM bridge and are never given to the relay.
use super::*;
use actr_relay_security::{
    ActorChannelEnrollment, ActorChannelIdentity, ActorChannelSession, ActorIdentityKey,
    IdentityKey, IdentityRequest, IssuedIdentity, IssuerKey, LocalIdentity, SecureSession,
    signing_key_id,
};
use js_sys::{Function, Uint8Array};
use wasm_bindgen::closure::Closure;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch, js_namespace = globalThis, js_name = __actrRelayTrustedRequest)]
    async fn trusted_request(
        base: &str,
        path: &str,
        body: JsValue,
        signaling: &str,
        proof: &str,
    ) -> Result<JsValue, JsValue>;
}

#[derive(Default)]
pub(super) struct Manager {
    enrollment: Mutex<()>,
    identity: RefCell<Option<LocalIdentity>>,
    /// Actor channel identity enrolled for the credential digests in
    /// `actor_binding`, mirroring the native relay client's cache binding: a
    /// renewed credential invalidates the enrolled Actor certificate.
    actor_identity: RefCell<Option<Rc<ActorChannelIdentity>>>,
    actor_binding: RefCell<Option<([u8; 32], [u8; 32])>>,
    /// Issuer key ids observed to never mint Actor channel certificates.
    /// Their channels run the legacy relay security until the identity
    /// expires and enrollment is retried.
    actor_legacy_issuers: RefCell<HashMap<u32, ()>>,
    /// Stable per-Worker tag bound into the Actor certificate.
    runtime_instance_id: RefCell<Option<String>>,
    issuers: RefCell<HashMap<u32, IssuerKey>>,
    channels: RefCell<HashMap<String, Rc<Channel>>>,
}
struct Channel {
    peer: String,
    session: Rc<RefCell<SecureSession>>,
    raw_send: Function,
    application_send: RefCell<Option<ApplicationSender>>,
}
type ApplicationSender = Closure<dyn FnMut(JsValue) -> Result<(), JsValue>>;
fn now() -> u64 {
    (js_sys::Date::now() / 1000.0) as u64
}
fn error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}
fn context(client: &str) -> Result<Rc<ClientContext>, JsValue> {
    CLIENTS
        .with(|cell| cell.borrow().get(client).cloned())
        .ok_or_else(|| error("Client not registered"))
}
async fn request<T: serde::de::DeserializeOwned>(
    ctx: &Rc<ClientContext>,
    path: &str,
    body: JsValue,
) -> Result<T, JsValue> {
    let (issuer, signaling, proof) = {
        let rt = ctx.runtime.lock().await;
        let proof = match body.as_string() {
            Some(text) => rt
                .issuer
                .proof_header(
                    actr_protocol::realm_membership::ProofOp::RelayIdentity,
                    &rt.realm,
                    text.as_bytes(),
                )
                .map_err(error)?,
            None => String::new(),
        };
        (
            rt.issuer.endpoint().to_owned(),
            rt.signaling_url.clone(),
            proof,
        )
    };
    let response = trusted_request(&issuer, path, body, &signaling, &proof).await?;
    serde_json::from_str(
        &response
            .as_string()
            .ok_or_else(|| error("Invalid trusted AIS response"))?,
    )
    .map_err(error)
}

/// Stable per-Worker instance tag bound into the Actor certificate. Not a
/// secret: it only distinguishes concurrent runtime instances.
fn runtime_instance_id(manager: &Manager) -> String {
    let mut cell = manager.runtime_instance_id.borrow_mut();
    if cell.is_none() {
        let now = js_sys::Date::now() as u64;
        let random = (js_sys::Math::random() * u32::MAX as f64) as u32;
        *cell = Some(format!("sw-{now:016x}{random:08x}"));
    }
    cell.clone().expect("instance id just initialized")
}

/// Credential digest and state digest the Actor enrollment binds, mirroring
/// the native relay client. The browser has no credential generation counter;
/// the credential digest itself changes on every renewal, which is what
/// forces re-enrollment.
async fn actor_binding_digests(
    ctx: &Rc<ClientContext>,
) -> Result<Option<([u8; 32], [u8; 32])>, JsValue> {
    let rt = ctx.runtime.lock().await;
    let Some(credential) = rt.credential.clone() else {
        return Ok(None);
    };
    let credential_digest =
        actr_relay_security::actor_credential_digest(&credential.encode_to_vec());
    let mut state_input = credential_digest.to_vec();
    state_input.extend_from_slice(&0u64.to_be_bytes());
    let state_digest = Sha256::digest(&state_input).into();
    Ok(Some((credential_digest, state_digest)))
}

/// Enrollment material for the browser's persistent Actor key, mirroring the
/// native relay client's `certified_identity`. `None` when no credential is
/// available yet.
async fn build_actor_enrollment(
    ctx: &Rc<ClientContext>,
    binding: &([u8; 32], [u8; 32]),
) -> Result<Option<(ActorIdentityKey, ActorChannelEnrollment)>, JsValue> {
    let rt = ctx.runtime.lock().await;
    let Some(credential) = rt.credential.clone() else {
        return Ok(None);
    };
    let claims = actr_protocol::IdentityClaims::decode(credential.claims.as_ref())
        .map_err(|e| error(format!("Invalid local credential claims: {e}")))?;
    let actor_key =
        super::SwRuntime::load_actor_key(&rt.platform, &rt.client_actr_type, &rt.realm).await?;
    let policy_digest = Sha256::digest(
        format!(
            "actrium.channel.policy.v1:{}",
            claims.actor_type.to_string_repr()
        )
        .as_bytes(),
    );
    Ok(Some((
        ActorIdentityKey::from_bytes(actor_key.private_key_bytes()),
        ActorChannelEnrollment {
            policy_digest: policy_digest.into(),
            actor_state_digest: binding.1,
            runtime_instance_id: runtime_instance_id(&ctx.relay_security),
            actor_credential: credential.encode_to_vec(),
            actor_credential_digest: binding.0,
        },
    )))
}

pub(super) async fn ensure_identity(ctx: &Rc<ClientContext>) -> Result<(u64, String), JsValue> {
    let _guard = ctx.relay_security.enrollment.lock().await;
    let endpoint = ctx
        .runtime
        .lock()
        .await
        .actor_id
        .as_ref()
        .ok_or_else(|| error("Client identity not ready"))?
        .to_string_repr();
    // One retry: an unbound Actor key is healed by reissuing the credential,
    // which establishes the binding Issuer-side; the refreshed credential
    // then carries the key claim the enrollment requires.
    for attempt in 0..2u8 {
        if let Some(cached) = cached_identity(ctx, &endpoint).await? {
            return Ok(cached);
        }
        let binding = actor_binding_digests(ctx).await?;
        let actor_material = match &binding {
            Some(binding) => build_actor_enrollment(ctx, binding).await?,
            None => None,
        };
        let key = IdentityKey::generate().map_err(error)?;
        let noise_for_actor = key.clone();
        let proof = {
            let rt = ctx.runtime.lock().await;
            let kv = rt
                .platform
                .secret_store(&rt.cred_kv_namespace())
                .await
                .map_err(error)?;
            kv.get("renewal_token")
                .await
                .map_err(error)?
                .ok_or_else(|| error("Missing private renewal proof"))?
        };
        let body = serde_json::to_string(&IdentityRequest {
            endpoint: endpoint.clone(),
            renewal_token: proof,
            public_key: key.public,
            admission_commitment: key.admission_commitment(),
            actor: actor_material
                .as_ref()
                .map(|(_, enrollment)| enrollment.clone()),
        })
        .map_err(error)?;
        let issued: IssuedIdentity =
            match request(ctx, "relay-identity", JsValue::from_str(&body)).await {
                Ok(issued) => issued,
                Err(enrollment_error) if attempt == 0 && actor_material.is_some() => {
                    let healed = ctx.runtime.lock().await.heal_actor_binding().await;
                    match healed {
                        Ok(true) => continue,
                        Ok(false) => return Err(enrollment_error),
                        Err(heal_error) => {
                            log::warn!("Actor key-binding heal failed: {heal_error:?}");
                            return Err(enrollment_error);
                        }
                    }
                }
                Err(enrollment_error) => return Err(enrollment_error),
            };
        let actor_certificate = issued.actor_certificate.clone();
        let identity = key.certify(issued, &endpoint, now()).map_err(error)?;
        let realm = ctx.runtime.lock().await.realm.clone();
        let actor_identity = match (actor_material, actor_certificate) {
            (Some((actor_identity_key, _)), Some(certificate)) => {
                match ActorChannelIdentity::certify(
                    noise_for_actor,
                    actor_identity_key,
                    certificate,
                    identity.issued.issuer.clone(),
                    &realm,
                    &endpoint,
                    now(),
                ) {
                    Ok(actor) => {
                        ctx.relay_security
                            .actor_legacy_issuers
                            .borrow_mut()
                            .remove(&identity.issued.issuer.key_id);
                        Some(Rc::new(actor))
                    }
                    Err(actor_error) => {
                        log::warn!(
                            "Actor channel certification rejected: {actor_error}; using the legacy relay channel"
                        );
                        None
                    }
                }
            }
            (Some(_), None) => {
                // The Issuer does not issue Actor channel certificates (a mock
                // or a deployment predating Actor channels). Degrade this
                // issuer to the legacy relay channel instead of failing every
                // peer connection; the marker is re-checked each identity
                // lifetime so an upgraded Issuer is picked up without a
                // Worker restart.
                log::warn!(
                    "Issuer returned no Actor channel certificate; using the legacy relay channel"
                );
                ctx.relay_security
                    .actor_legacy_issuers
                    .borrow_mut()
                    .insert(identity.issued.issuer.key_id, ());
                None
            }
            (None, _) => None,
        };
        *ctx.relay_security.actor_binding.borrow_mut() = if actor_identity.is_some() {
            binding
        } else {
            None
        };
        *ctx.relay_security.actor_identity.borrow_mut() = actor_identity;
        {
            let mut keys = ctx.relay_security.issuers.borrow_mut();
            keys.retain(|_, key| key.expires_at > now());
            if keys.len() >= 256 && !keys.contains_key(&identity.issued.issuer.key_id) {
                return Err(error("Issuer cache capacity exceeded"));
            }
            keys.insert(
                identity.issued.issuer.key_id,
                identity.issued.issuer.clone(),
            );
        }
        let result = (
            identity.expires_at,
            bytes_to_base64(&identity.issued.certificate.claims),
        );
        *ctx.relay_security.identity.borrow_mut() = Some(identity);
        return Ok(result);
    }
    Err(error("Relay identity enrollment did not converge"))
}

/// Serve the identity from cache when it is still valid and consistent with
/// the current credential. Returns `None` when enrollment must run.
async fn cached_identity(
    ctx: &Rc<ClientContext>,
    endpoint: &str,
) -> Result<Option<(u64, String)>, JsValue> {
    let cached = ctx
        .relay_security
        .identity
        .borrow()
        .as_ref()
        .filter(|identity| identity.endpoint == endpoint && identity.expires_at > now() + 30)
        .map(|identity| {
            (
                identity.expires_at,
                identity.issued.issuer.key_id,
                bytes_to_base64(&identity.issued.certificate.claims),
            )
        });
    let Some((expires_at, issuer_key_id, claims_b64)) = cached else {
        return Ok(None);
    };
    let binding = *ctx.relay_security.actor_binding.borrow();
    let binding_current = actor_binding_digests(ctx).await?;
    let actor_ok = match (&binding_current, &binding) {
        (Some((credential_digest, state_digest)), Some((cached, cached_state))) => {
            cached == credential_digest
                && cached_state == state_digest
                && ctx
                    .relay_security
                    .actor_identity
                    .borrow()
                    .as_ref()
                    .is_some_and(|actor| actor.expires_at() > now() + 30)
        }
        (Some(_), None) => false,
        (None, _) => true,
    };
    let legacy_issuer = ctx
        .relay_security
        .actor_legacy_issuers
        .borrow()
        .contains_key(&issuer_key_id);
    if actor_ok || legacy_issuer {
        return Ok(Some((expires_at, claims_b64)));
    }
    Ok(None)
}
async fn issuer(ctx: &Rc<ClientContext>, id: u32) -> Result<IssuerKey, JsValue> {
    if let Some(key) = ctx
        .relay_security
        .issuers
        .borrow()
        .get(&id)
        .filter(|key| key.expires_at > now())
        .cloned()
    {
        return Ok(key);
    }
    let realm = ctx.runtime.lock().await.realm.clone();
    let key: IssuerKey = request(ctx, &format!("relay-key/{realm}/{id}"), JsValue::NULL).await?;
    if key.key_id != id || key.expires_at <= now() {
        return Err(error("Invalid trusted issuer"));
    }
    let mut keys = ctx.relay_security.issuers.borrow_mut();
    keys.retain(|_, key| key.expires_at > now());
    if keys.len() >= 256 {
        return Err(error("Issuer cache capacity exceeded"));
    }
    keys.insert(id, key.clone());
    Ok(key)
}

#[wasm_bindgen]
pub async fn prepare_relay_connection(
    client_id: String,
    endpoint: String,
    channel: String,
    send: Function,
    initiator: bool,
) -> Result<String, JsValue> {
    let peer = relay_peer_identity(client_id.clone(), endpoint).await?;
    let ctx = context(&client_id)?;
    let number = channel
        .rsplit(':')
        .next()
        .ok_or_else(|| error("Invalid channel"))?
        .parse::<u64>()
        .map_err(error)?;
    let session = {
        let actor = ctx.relay_security.actor_identity.borrow().clone();
        match actor.as_deref() {
            Some(actor) if initiator => {
                ActorChannelSession::new_front(actor, &peer, &number.to_string(), now())
                    .map_err(error)?
                    .into_inner()
            }
            Some(actor) => {
                ActorChannelSession::new_service(actor, &peer, &number.to_string(), now())
                    .map_err(error)?
                    .into_inner()
            }
            None => {
                let identity = ctx.relay_security.identity.borrow();
                SecureSession::new(
                    identity
                        .as_ref()
                        .ok_or_else(|| error("Missing certified local key"))?,
                    &peer,
                    number,
                    now(),
                )
                .map_err(error)?
            }
        }
    };
    if ctx.relay_security.channels.borrow().len() >= 256 {
        return Err(error("Secure channel capacity exceeded"));
    }
    ctx.relay_security.channels.borrow_mut().insert(
        channel,
        Rc::new(Channel {
            peer: peer.clone(),
            session: Rc::new(RefCell::new(session)),
            raw_send: send,
            application_send: RefCell::new(None),
        }),
    );
    Ok(peer)
}
#[wasm_bindgen]
pub fn start_relay_handshake(client_id: String, channel: String) -> Result<bool, JsValue> {
    let ctx = context(&client_id)?;
    let entry = ctx
        .relay_security
        .channels
        .borrow()
        .get(&channel)
        .cloned()
        .ok_or_else(|| error("Unknown encrypted channel"))?;
    let first = entry.session.borrow_mut().start().map_err(error)?;
    if let Some(first) = first {
        entry
            .raw_send
            .call1(&JsValue::NULL, &Uint8Array::from(first.as_slice()))?;
    }
    Ok(false)
}

#[wasm_bindgen]
pub async fn handle_relay_record(
    client_id: String,
    channel: String,
    payload: Vec<u8>,
) -> Result<bool, JsValue> {
    let ctx = context(&client_id)?;
    let entry = ctx
        .relay_security
        .channels
        .borrow()
        .get(&channel)
        .cloned()
        .ok_or_else(|| error("Unknown encrypted channel"))?;
    let key = if let Some(id) = signing_key_id(&payload).map_err(error)? {
        Some(issuer(&ctx, id).await?)
    } else {
        None
    };
    let received = entry
        .session
        .borrow_mut()
        .receive(&payload, key.as_ref(), now())
        .map_err(error)?;
    let buffered: usize = ctx
        .relay_security
        .channels
        .borrow()
        .values()
        .map(|entry| entry.session.borrow().buffered_bytes())
        .sum();
    if buffered > 16 * 1024 * 1024 {
        entry.session.borrow_mut().close();
        return Err(error("Encrypted receive budget exceeded"));
    }
    for record in received.outbound {
        entry
            .raw_send
            .call1(&JsValue::NULL, &Uint8Array::from(record.as_slice()))?;
    }
    if received.became_ready {
        let session = entry.session.clone();
        let raw = entry.raw_send.clone();
        let send = Closure::wrap(Box::new(move |payload: JsValue| -> Result<(), JsValue> {
            let application = Uint8Array::new(&payload).to_vec();
            let records = session
                .borrow_mut()
                .send(&application, now())
                .map_err(error);
            let result = records.and_then(|records| {
                for record in records {
                    raw.call1(&JsValue::NULL, &Uint8Array::from(record.as_slice()))?;
                }
                Ok(())
            });
            if result.is_err() {
                session.borrow_mut().close();
                let _ = raw.call1(&JsValue::NULL, &JsValue::NULL);
            }
            result
        }) as Box<dyn FnMut(JsValue) -> Result<(), JsValue>>);
        let function: Function = send.as_ref().unchecked_ref::<Function>().clone();
        *entry.application_send.borrow_mut() = Some(send);
        register_relay_connection(
            client_id.clone(),
            entry.peer.clone(),
            channel.clone(),
            function,
        )
        .await?;
    }
    if let Some(application) = received.application {
        if application.len() < 5
            || application[0] > 3
            || u32::from_be_bytes(application[1..5].try_into().unwrap()) as usize
                != application.len() - 5
        {
            return Err(error("Invalid encrypted application frame"));
        }
        let value = js_sys::Object::new();
        js_sys::Reflect::set(
            &value,
            &"streamId".into(),
            &format!("{}:{}", entry.peer, application[0]).into(),
        )?;
        js_sys::Reflect::set(&value, &"data".into(), &Uint8Array::from(&application[5..]))?;
        handle_relay_data(client_id, value.into()).await?;
    }
    Ok(entry.session.borrow().is_ready())
}

pub(super) fn authenticated(ctx: &ClientContext, channel: &str, peer: &str) -> bool {
    ctx.relay_security
        .channels
        .borrow()
        .get(channel)
        .is_some_and(|entry| entry.peer == peer && entry.session.borrow().is_ready())
}
pub(super) fn access_token(ctx: &ClientContext) -> Result<String, JsValue> {
    let identity = ctx.relay_security.identity.borrow();
    let identity = identity
        .as_ref()
        .ok_or_else(|| error("Missing certified local key"))?;
    Ok(bytes_to_base64(
        &serde_json::to_vec(&identity.relay_credential()).map_err(error)?,
    ))
}
pub(super) fn remove(ctx: &ClientContext, channel: &str) {
    if let Some(entry) = ctx.relay_security.channels.borrow_mut().remove(channel) {
        entry.session.borrow_mut().close();
    }
}
