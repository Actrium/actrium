pub const PACKAGE_NAME: &str = "actrium";
pub const PACKAGE_DESCRIPTION: &str = "Actrium browser client app";
pub const PACKAGE_AUTHOR: &str = "Actrium-Team";
pub const PACKAGE_LICENSE: &str = "Apache-2.0";
pub const PACKAGE_TAG_DEV: &str = "dev";
pub const PACKAGE_TAG_CLIENT: &str = "client";

pub const CLIENT_ACTR_PUBLISHER: &str = "actro";
pub const CLIENT_ACTR_NAME: &str = "actro";
pub const DEPENDENCY_NAME: &str = "bid-service";

pub const DEFAULT_SIGNALING_URL: &str = "wss://loran.example.com/signaling/ws";
pub const SIGNALING_PATH: &str = "/signaling/ws";
pub const OBSERVABILITY_FILTER_LEVEL: &str = "debug";
pub const OBSERVABILITY_TRACING_ENDPOINT: &str = "http://127.0.0.1:4317";

pub const SCRIPT_DEV: &str = "xcodegen generate";
pub const SCRIPT_TEST: &str = "xcodebuild test";
pub const ACL_PERMISSION_ALLOW: &str = "allow";

pub const LOCK_METADATA_VERSION: u32 = 1;
pub const LOCK_GENERATED_AT: &str = "2026-02-06T06:40:17.912658+00:00";
pub const LOCK_DEPENDENCY_NAME: &str = "bot";
pub const LOCK_DEPENDENCY_FINGERPRINT: &str =
    "service_semantic:090a8d3fb0c937ac019606c43cbe40e2223690e3d3ad87b2f16e42119c41820a";
pub const LOCK_CACHED_AT: &str = "2026-02-06T06:40:17.912656+00:00";
pub const LOCK_DEPENDENCY_FILE_PATH: &str = "bot/actrium.proto";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActrServiceEndpoint {
    pub domain_name: String,
    pub port_info: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActrTomlConfig {
    pub publisher: String,
    pub name: String,
    pub version: String,
    pub realm: String,
    pub signaling_services: Vec<ActrServiceEndpoint>,
    pub stun_services: Vec<ActrServiceEndpoint>,
    pub turn_services: Vec<ActrServiceEndpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActrLockTomlConfig {
    pub publisher: String,
    pub name: String,
    pub version: String,
}

pub fn endpoint_host(endpoint: &ActrServiceEndpoint) -> String {
    match endpoint
        .port_info
        .as_deref()
        .filter(|port| !port.is_empty())
    {
        Some(port) => format!("{}:{}", endpoint.domain_name, port),
        None => endpoint.domain_name.clone(),
    }
}

pub fn normalize_signaling_url(endpoint: Option<&ActrServiceEndpoint>) -> String {
    let Some(endpoint) = endpoint else {
        return DEFAULT_SIGNALING_URL.to_string();
    };

    let mut url = format!("{}{}", endpoint_host(endpoint), SIGNALING_PATH);
    if !url.starts_with("ws") {
        url = format!("wss://{url}");
    }
    url
}

pub fn normalize_stun_url(endpoint: &ActrServiceEndpoint) -> String {
    let mut url = endpoint_host(endpoint);
    if !url.starts_with("stun:") {
        url = format!("stun:{url}");
    }
    url
}

pub fn normalize_turn_url(endpoint: &ActrServiceEndpoint) -> String {
    let mut url = endpoint_host(endpoint);
    if !url.starts_with("turn:") {
        url = format!("turn:{url}");
    }
    url
}

pub fn render_actr_toml(config: &ActrTomlConfig) -> String {
    let server_actr_type = format!("{}/{}@{}", config.publisher, config.name, config.version);
    let signaling_url = normalize_signaling_url(config.signaling_services.first());
    let stun_urls = config
        .stun_services
        .iter()
        .map(|service| format!("\"{}\"", normalize_stun_url(service)))
        .collect::<Vec<_>>()
        .join(", ");
    let turn_urls = config
        .turn_services
        .iter()
        .map(|service| format!("\"{}\"", normalize_turn_url(service)))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        r#"edition = 1
exports = []

[package]
name = "{package_name}"
description = "{package_description}"
authors = ["{package_author}"]
license = "{package_license}"
tags = ["{package_tag_dev}", "{package_tag_client}"]

[package.actr_type]
publisher = "{client_actr_publisher}"
name = "{client_actr_name}"

[dependencies]
{dependency_name} = {{ actr_type = "{server_actr_type}" }}

[system.signaling]
url = "{signaling_url}"

[system.deployment]
realm = "{realm}"

[system.discovery]
visible = true

[system.observability]
filter_level = "{observability_filter_level}"
tracing_enabled = true
tracing_endpoint = "{observability_tracing_endpoint}"
tracing_service_name = "{package_name}"

[system.webrtc]
force_relay = false
stun_urls = [{stun_urls}]
turn_urls = [{turn_urls}]

[scripts]
dev = "{script_dev}"
test = "{script_test}"

[acl]

[[acl.rules]]
permission = "{acl_permission_allow}"
types = ["{server_actr_type}"]
"#,
        package_name = PACKAGE_NAME,
        package_description = PACKAGE_DESCRIPTION,
        package_author = PACKAGE_AUTHOR,
        package_license = PACKAGE_LICENSE,
        package_tag_dev = PACKAGE_TAG_DEV,
        package_tag_client = PACKAGE_TAG_CLIENT,
        client_actr_publisher = CLIENT_ACTR_PUBLISHER,
        client_actr_name = CLIENT_ACTR_NAME,
        dependency_name = DEPENDENCY_NAME,
        server_actr_type = server_actr_type,
        signaling_url = signaling_url,
        realm = config.realm,
        observability_filter_level = OBSERVABILITY_FILTER_LEVEL,
        observability_tracing_endpoint = OBSERVABILITY_TRACING_ENDPOINT,
        stun_urls = stun_urls,
        turn_urls = turn_urls,
        script_dev = SCRIPT_DEV,
        script_test = SCRIPT_TEST,
        acl_permission_allow = ACL_PERMISSION_ALLOW,
    )
}

pub fn render_actr_lock_toml(config: &ActrLockTomlConfig) -> String {
    format!(
        r#"[metadata]
version = {lock_metadata_version}
generated_at = "{lock_generated_at}"

[[dependency]]
name = "{lock_dependency_name}"
actr_type = "{server_publisher}/{server_name}@{server_version}"
fingerprint = "{lock_dependency_fingerprint}"
cached_at = "{lock_cached_at}"

[[dependency.files]]
path = "{lock_dependency_file_path}"
fingerprint = ""
"#,
        lock_metadata_version = LOCK_METADATA_VERSION,
        lock_generated_at = LOCK_GENERATED_AT,
        lock_dependency_name = LOCK_DEPENDENCY_NAME,
        server_publisher = config.publisher,
        server_name = config.name,
        server_version = config.version,
        lock_dependency_fingerprint = LOCK_DEPENDENCY_FINGERPRINT,
        lock_cached_at = LOCK_CACHED_AT,
        lock_dependency_file_path = LOCK_DEPENDENCY_FILE_PATH,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(domain_name: &str, port_info: Option<&str>) -> ActrServiceEndpoint {
        ActrServiceEndpoint {
            domain_name: domain_name.to_string(),
            port_info: port_info.map(ToOwned::to_owned),
        }
    }

    #[test]
    fn normalizes_actr_service_urls_like_runtime_typescript() {
        assert_eq!(normalize_signaling_url(None), DEFAULT_SIGNALING_URL);
        assert_eq!(
            normalize_signaling_url(Some(&endpoint("loran.example.com", Some("443")))),
            "wss://loran.example.com:443/signaling/ws"
        );
        assert_eq!(
            normalize_signaling_url(Some(&endpoint("ws://local.test", None))),
            "ws://local.test/signaling/ws"
        );
        assert_eq!(
            normalize_stun_url(&endpoint("loran.example.com", Some("3478"))),
            "stun:loran.example.com:3478"
        );
        assert_eq!(
            normalize_turn_url(&endpoint("turn:loran.example.com", Some("3478"))),
            "turn:loran.example.com:3478"
        );
    }

    #[test]
    fn renders_actr_toml_with_existing_runtime_shape() {
        let toml = render_actr_toml(&ActrTomlConfig {
            publisher: "acmeopenclaw".to_string(),
            name: "AskService".to_string(),
            version: "1.0.0".to_string(),
            realm: "k7m2q9xh@example.com".to_string(),
            signaling_services: vec![endpoint("loran.example.com", Some("443"))],
            stun_services: vec![endpoint("loran.example.com", Some("3478"))],
            turn_services: vec![endpoint("loran.example.com", Some("3478"))],
        });

        assert!(toml.contains("name = \"actrium\""));
        assert!(toml.contains("tracing_service_name = \"actrium\""));
        assert!(toml.contains("bid-service = { actr_type = \"acmeopenclaw/AskService@1.0.0\" }"));
        assert!(toml.contains("url = \"wss://loran.example.com:443/signaling/ws\""));
        assert!(toml.contains("realm = \"k7m2q9xh@example.com\""));
        assert!(toml.contains("types = [\"acmeopenclaw/AskService@1.0.0\"]"));
        assert!(toml.contains("stun_urls = [\"stun:loran.example.com:3478\"]"));
        assert!(toml.contains("turn_urls = [\"turn:loran.example.com:3478\"]"));
        assert!(toml.ends_with('\n'));
    }

    #[test]
    fn renders_lock_toml_with_existing_runtime_shape() {
        let toml = render_actr_lock_toml(&ActrLockTomlConfig {
            publisher: "acmeopenclaw".to_string(),
            name: "AskService".to_string(),
            version: "1.0.0".to_string(),
        });

        assert!(toml.contains("version = 1"));
        assert!(toml.contains("name = \"bot\""));
        assert!(toml.contains("actr_type = \"acmeopenclaw/AskService@1.0.0\""));
        assert!(toml.contains("path = \"bot/actrium.proto\""));
        assert!(toml.ends_with('\n'));
    }
}
