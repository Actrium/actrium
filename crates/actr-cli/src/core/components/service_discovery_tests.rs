use super::*;

fn sample_context(realm_client_key: Option<&str>) -> DiscoveryContext {
    DiscoveryContext {
        package_actr_type: ActrType {
            publisher: "acme".to_string(),
            name: "cli-client".to_string(),
            version: "1.0.0".to_string(),
        },
        signaling_url: Url::parse("ws://localhost:8081/signaling/ws").unwrap(),
        issuer_endpoint: "http://localhost:8081/issuer".to_string(),
        realm: Some("test@example.com".to_string()),
        realm_client_key: realm_client_key.map(std::path::PathBuf::from),
    }
}

fn sample_actor_id() -> ActrId {
    ActrId {
        serial_number: 42,
        realm: "test@example.com".to_string(),
    }
}

fn sample_credential() -> AIdCredential {
    AIdCredential {
        key_id: 7,
        claims: vec![1, 2, 3, 4].into(),
        signature: vec![5, 6, 7, 8].into(),
    }
}

#[test]
fn build_signaling_request_with_identity_uses_url_query_params() {
    let signaling_url = Url::parse("ws://localhost:8081/signaling/ws?existing=1").unwrap();
    let actor_id = sample_actor_id();
    let credential = sample_credential();

    let request = NetworkServiceDiscovery::build_signaling_request_with_identity(
        &signaling_url,
        &actor_id,
        &credential,
    )
    .unwrap();

    let parsed = Url::parse(&request.uri().to_string()).unwrap();
    let params: std::collections::HashMap<String, String> =
        parsed.query_pairs().into_owned().collect();
    assert_eq!(params["existing"], "1", "existing params are preserved");
    assert_eq!(params["actor_id"], actor_id.to_string_repr());
    assert_eq!(params["key_id"], "7");
    assert_eq!(params["claims"], "AQIDBA==");
    assert_eq!(params["signature"], "BQYHCA==");
    assert!(
        request.headers().get("x-actr-id").is_none(),
        "identity must not travel in headers"
    );
}

#[test]
fn cli_discovery_register_request_uses_linked_auth_mode() {
    let discovery = NetworkServiceDiscovery::new(sample_context(Some("rs_test_secret")));
    let request = discovery.build_linked_register_request(discovery.required_realm().unwrap());

    assert_eq!(request.auth_mode, Some(RegisterAuthMode::Linked as i32));
    assert_eq!(request.manifest_raw, None);
    assert_eq!(request.publisher_signature, None);
    assert_eq!(request.target, None);
    assert_eq!(request.actr_type.name, "cli-client");
    assert_eq!(request.realm, "test@example.com");
}

#[test]
fn cli_discovery_requires_realm() {
    let mut context = sample_context(Some("rs_test_secret"));
    context.realm = None;
    let err = NetworkServiceDiscovery::new(context)
        .required_realm()
        .unwrap_err();
    assert!(err.to_string().contains("network.realm is required"));
}

#[test]
fn cli_discovery_requires_a_realm_client_key() {
    let missing = NetworkServiceDiscovery::new(sample_context(None));
    let err = missing.required_client_key("test@example.com").unwrap_err();
    assert!(
        err.to_string()
            .contains("network.realm_client_key is required")
    );

    let absent = NetworkServiceDiscovery::new(sample_context(Some("/nonexistent/key.json")));
    let err = absent.required_client_key("test@example.com").unwrap_err();
    assert!(err.to_string().contains("Realm client key"));
}

#[test]
fn lookup_name_accepts_registry_key_service_name_and_full_actor_type() {
    let entry = discovery_response::TypeEntry {
        actr_type: ActrType {
            publisher: "acme".to_string(),
            name: "EchoService".to_string(),
            version: "1.0.0".to_string(),
        },
        name: "acme:EchoService".to_string(),
        description: None,
        service_fingerprint: "service_semantic:echo".to_string(),
        published_at: None,
        tags: vec![],
    };

    assert!(NetworkServiceDiscovery::matches_lookup_name(
        &entry,
        "acme:EchoService"
    ));
    assert!(NetworkServiceDiscovery::matches_lookup_name(
        &entry,
        "EchoService"
    ));
    assert!(NetworkServiceDiscovery::matches_lookup_name(
        &entry,
        "acme/EchoService@1.0.0"
    ));
    assert!(!NetworkServiceDiscovery::matches_lookup_name(
        &entry,
        "OtherService"
    ));
}
