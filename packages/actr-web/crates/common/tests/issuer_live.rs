#![cfg(target_arch = "wasm32")]

use actr_protocol::realm_membership::RealmClientKeyFile;
use actr_protocol::{
    ActrType, RegisterAuthMode, RegisterRequest, RenewCredentialRequest, register_response,
    renew_credential_response,
};
use actr_web_common::issuer_client::{WebIssuerClient, WebRealmClientKey};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

async fn get(url: &str) -> web_sys::Response {
    let global: web_sys::WorkerGlobalScope = js_sys::global().unchecked_into();
    JsFuture::from(global.fetch_with_str(url))
        .await
        .unwrap()
        .dyn_into()
        .unwrap()
}

#[wasm_bindgen_test]
async fn authorized_web_client_registers_and_renews_with_fresh_proofs() {
    let endpoint = option_env!("ACTR_WEB_ISSUER_URL").expect(
        "run packages/actr-web/scripts/test-issuer-proof.sh to start the real Issuer fixture",
    );
    let response = get(&format!("{endpoint}/fixture")).await;
    let text = JsFuture::from(response.text().unwrap())
        .await
        .unwrap()
        .as_string()
        .unwrap();
    let (file, _) = RealmClientKeyFile::parse(&text).unwrap();
    let signer = WebRealmClientKey::from_key_file(endpoint, &file.realm, &text).unwrap();
    let client = WebIssuerClient::new(endpoint, signer);
    let request = RegisterRequest {
        actr_type: ActrType::from_string_repr("linked-src/source-workload@1.0.0").unwrap(),
        realm: file.realm.clone(),
        service_spec: None,
        acl: None,
        service: None,
        ws_address: None,
        manifest_raw: None,
        publisher_signature: None,
        target: None,
        signing_public_key: None,
        auth_mode: Some(RegisterAuthMode::Linked as i32),
    };
    let result = client.register_linked(request).await.unwrap();
    let register_response::Result::Success(registered) = result.result.unwrap() else {
        panic!("registration rejected")
    };
    assert!(client.accepts_credential(&registered.credential));
    let renewed = client
        .renew_credential(RenewCredentialRequest {
            actr_id: registered.actr_id.clone(),
            renewal_token: registered.renewal_token.unwrap(),
            attempt_id: uuid::Uuid::new_v4().as_bytes().to_vec().into(),
        })
        .await
        .unwrap();
    let renew_credential_response::Result::Success(renewed) = renewed.result.unwrap() else {
        panic!("renewal rejected")
    };
    assert_eq!(renewed.actr_id, registered.actr_id);
    assert!(client.accepts_credential(&renewed.credential));
    assert!(
        renewed
            .turn_credential
            .username
            .contains(&format!(":client_key={}:actor=", file.key_id))
    );
    assert!(get(&format!("{endpoint}/stop")).await.ok());
}
