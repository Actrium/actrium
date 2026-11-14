use super::*;
use platform::config::realm::{DevJoinMode, DevJoinRole};
use sha2::{Digest, Sha256};

#[tokio::test]
async fn supervised_config_saves_cannot_change_admission_but_standalone_saves_can() {
    let mut config =
        LoranConfig::from_toml(include_str!("../../../../loran/config/loran.example.toml"))
            .unwrap();
    config.env = "dev".into();
    let fixture_secret = |label: &str| hex::encode(Sha256::digest(label.as_bytes()));
    config.loran_shared_key = fixture_secret("test-shared");
    config.turn.turn_secret = fixture_secret("test-turn");
    config
        .services
        .issuer
        .as_mut()
        .unwrap()
        .server
        .renewal_token_secret = fixture_secret("test-renewal");
    config.control.admin_ui.enabled = true;
    // Public test fixtures; this test never starts either control listener.
    config.control.admin_ui.password_hash = "$argon2id$v=19$m=65536,t=2,p=1$YWN0cml4LXRlc3Qtc2FsdA$lfmjnm9CjoHQC9U79KOWXJkyWTUA7g5MhgG2DDPqe1E".into();
    config.control.grpc_api.enabled = true;
    config.control.grpc_api.shared_secret = fixture_secret("test-supervisor");
    assert!(config.superv_managed());
    let path = std::env::temp_dir().join(format!(
        "loran-admin-admission-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let original = config.to_toml().unwrap();
    tokio::fs::write(&path, &original).await.unwrap();
    let service = AdminApiService::new("node", "node", "test", "test", ServiceCollector::new())
        .unwrap()
        .with_config_path(path.clone())
        .with_running_config(config.clone());
    assert_eq!(
        service
            .get_service_detail_direct("issuer")
            .await
            .unwrap()
            .config
            .unwrap()["dev_join_writable"],
        false
    );
    let mut changed = config.clone();
    changed.realm.dev_join.mode = DevJoinMode::Open;
    let error = service
        .save_config_file_direct(&changed.to_toml().unwrap(), true)
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::PermissionDenied);
    assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), original);
    changed.realm.dev_join = config.realm.dev_join;
    changed.realm.dev_join.default_role = DevJoinRole::Viewer;
    assert_eq!(
        service
            .save_config_file_direct(&changed.to_toml().unwrap(), true)
            .await
            .unwrap_err()
            .code(),
        tonic::Code::PermissionDenied
    );
    assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), original);
    changed = config.clone();
    changed.name = "metadata-only".into();
    service
        .save_config_file_direct(&changed.to_toml().unwrap(), true)
        .await
        .unwrap();
    config.control.grpc_api.enabled = false;
    assert!(!config.superv_managed());
    tokio::fs::write(&path, config.to_toml().unwrap())
        .await
        .unwrap();
    let standalone = service.with_running_config(config.clone());
    assert_eq!(
        standalone
            .get_service_detail_direct("issuer")
            .await
            .unwrap()
            .config
            .unwrap()["dev_join_writable"],
        true
    );
    config.realm.dev_join.mode = DevJoinMode::Open;
    standalone
        .save_config_file_direct(&config.to_toml().unwrap(), false)
        .await
        .unwrap();
    assert_eq!(
        LoranConfig::from_toml(&tokio::fs::read_to_string(&path).await.unwrap())
            .unwrap()
            .realm
            .dev_join
            .mode,
        DevJoinMode::Open
    );
    tokio::fs::remove_file(path).await.unwrap();
}
