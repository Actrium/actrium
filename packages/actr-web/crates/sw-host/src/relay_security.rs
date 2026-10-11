//! Worker-only end-to-end security. Enrollment proofs and private keys never
//! cross the DOM bridge and are never given to the relay.
use super::*;
use actr_relay_security::{
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
    if let Some(identity) = ctx
        .relay_security
        .identity
        .borrow()
        .as_ref()
        .filter(|identity| identity.endpoint == endpoint && identity.expires_at > now() + 30)
    {
        return Ok((
            identity.expires_at,
            bytes_to_base64(&identity.issued.certificate.claims),
        ));
    }
    let key = IdentityKey::generate().map_err(error)?;
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
    })
    .map_err(error)?;
    let issued: IssuedIdentity = request(ctx, "relay-identity", JsValue::from_str(&body)).await?;
    let identity = key.certify(issued, &endpoint, now()).map_err(error)?;
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
    Ok(result)
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
