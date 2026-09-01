use super::*;
use tempfile::TempDir;

#[tokio::test]
async fn get_cached_proto_returns_none_when_path_absent() {
    let dir = TempDir::new().unwrap();
    let mgr = DefaultCacheManager::with_project_root(dir.path().to_path_buf());
    assert!(mgr.get_cached_proto("nonexistent").await.unwrap().is_none());
}

#[tokio::test]
async fn cache_get_invalidate_roundtrip() {
    let dir = TempDir::new().unwrap();
    let mgr = DefaultCacheManager::with_project_root(dir.path().to_path_buf());

    // Cache a proto file.
    let proto = ProtoFile {
        name: "echo.proto".into(),
        path: "echo.proto".into(),
        content: "syntax = \"proto3\";".into(),
        services: vec![],
    };
    mgr.cache_proto("echo", &[proto]).await.unwrap();

    // Now retrieve it.
    let cached = mgr.get_cached_proto("echo").await.unwrap().unwrap();
    assert_eq!(cached.files.len(), 1);
    assert_eq!(cached.files[0].name, "echo.proto");
    assert!(!cached.fingerprint.value.is_empty());

    // Invalidate.
    mgr.invalidate_cache("echo").await.unwrap();
    assert!(mgr.get_cached_proto("echo").await.unwrap().is_none());
}

#[tokio::test]
async fn cache_proto_adds_dot_proto_extension_when_missing() {
    let dir = TempDir::new().unwrap();
    let mgr = DefaultCacheManager::with_project_root(dir.path().to_path_buf());
    let proto = ProtoFile {
        name: "echo".into(),
        path: "echo".into(),
        content: "syntax = \"proto3\";".into(),
        services: vec![],
    };
    mgr.cache_proto("echo", &[proto]).await.unwrap();
    let cached = mgr.get_cached_proto("echo").await.unwrap().unwrap();
    // The name field is unchanged, but the file on disk has .proto appended.
    assert_eq!(cached.files.len(), 1);
    assert!(cached.files[0].name.ends_with(".proto"));
}

#[tokio::test]
async fn cache_proto_prunes_stale_files_for_the_same_alias() {
    let tmp = tempfile::tempdir().unwrap();
    let mgr = DefaultCacheManager::with_project_root(tmp.path().to_path_buf());
    let old = ProtoFile {
        name: "old.proto".into(),
        path: PathBuf::new(),
        content: "syntax = \"proto3\";".into(),
        services: vec![],
    };
    let current = ProtoFile {
        name: "current.proto".into(),
        path: PathBuf::new(),
        content: "syntax = \"proto3\";".into(),
        services: vec![],
    };

    mgr.cache_proto("echo", &[old]).await.unwrap();
    mgr.cache_proto("echo", &[current]).await.unwrap();

    let dir = tmp.path().join("protos/remote/echo");
    assert!(!dir.join("old.proto").exists());
    assert!(dir.join("current.proto").exists());
}

#[tokio::test]
async fn cache_proto_isolates_aliases_that_share_a_registry_name() {
    let tmp = tempfile::tempdir().unwrap();
    let mgr = DefaultCacheManager::with_project_root(tmp.path().to_path_buf());
    let first = ProtoFile {
        name: "service.proto".into(),
        path: PathBuf::new(),
        content: "// primary".into(),
        services: vec![],
    };
    let second = ProtoFile {
        name: "service.proto".into(),
        path: PathBuf::new(),
        content: "// secondary".into(),
        services: vec![],
    };

    mgr.cache_proto("primary", &[first]).await.unwrap();
    mgr.cache_proto("secondary", &[second]).await.unwrap();

    assert_eq!(
        std::fs::read_to_string(tmp.path().join("protos/remote/primary/service.proto")).unwrap(),
        "// primary"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("protos/remote/secondary/service.proto")).unwrap(),
        "// secondary"
    );
}

#[tokio::test]
async fn clear_cache_removes_protos_directory() {
    let dir = TempDir::new().unwrap();
    let mgr = DefaultCacheManager::with_project_root(dir.path().to_path_buf());
    let proto = ProtoFile {
        name: "echo.proto".into(),
        path: "echo.proto".into(),
        content: "// stub".into(),
        services: vec![],
    };
    mgr.cache_proto("echo", &[proto]).await.unwrap();
    assert!(dir.path().join("protos").exists());

    mgr.clear_cache().await.unwrap();
    assert!(!dir.path().join("protos").exists());
}

#[tokio::test]
async fn get_cache_stats_counts_entries_and_sizes() {
    let dir = TempDir::new().unwrap();
    let mgr = DefaultCacheManager::with_project_root(dir.path().to_path_buf());
    let proto = ProtoFile {
        name: "echo.proto".into(),
        path: "echo.proto".into(),
        content: "// hi".into(),
        services: vec![],
    };
    mgr.cache_proto("echo", &[proto]).await.unwrap();
    let stats = mgr.get_cache_stats().await.unwrap();
    assert_eq!(stats.total_entries, 1);
    assert!(stats.total_size_bytes > 0);
    assert_eq!(stats.hit_rate, 0.0);
    assert_eq!(stats.miss_rate, 0.0);
}

#[tokio::test]
async fn cache_rejects_path_traversal_without_touching_existing_snapshot() {
    let dir = TempDir::new().unwrap();
    let mgr = DefaultCacheManager::with_project_root(dir.path().to_path_buf());
    let proto = ProtoFile {
        name: "echo.proto".into(),
        path: PathBuf::new(),
        content: "// safe".into(),
        services: vec![],
    };
    mgr.cache_proto("echo", &[proto]).await.unwrap();

    assert!(
        mgr.cache_proto(
            "../../outside",
            &[ProtoFile {
                name: "outside.proto".into(),
                path: PathBuf::new(),
                content: "// unsafe".into(),
                services: vec![],
            }],
        )
        .await
        .is_err()
    );
    let malicious = ProtoFile {
        name: "../outside.proto".into(),
        path: PathBuf::new(),
        content: "// unsafe".into(),
        services: vec![],
    };
    assert!(mgr.cache_proto("echo", &[malicious]).await.is_err());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("protos/remote/echo/echo.proto")).unwrap(),
        "// safe"
    );
    assert!(!dir.path().join("outside.proto").exists());
}

#[tokio::test]
async fn retain_aliases_prunes_orphan_dependency_directories() {
    let dir = TempDir::new().unwrap();
    let mgr = DefaultCacheManager::with_project_root(dir.path().to_path_buf());
    for alias in ["keep", "remove"] {
        mgr.cache_proto(
            alias,
            &[ProtoFile {
                name: "service.proto".into(),
                path: PathBuf::new(),
                content: "// proto".into(),
                services: vec![],
            }],
        )
        .await
        .unwrap();
    }
    mgr.retain_aliases(&std::collections::HashSet::from(["keep".to_string()]))
        .await
        .unwrap();
    assert!(dir.path().join("protos/remote/keep").exists());
    assert!(!dir.path().join("protos/remote/remove").exists());
}

#[tokio::test]
async fn retaining_an_empty_alias_set_prunes_all_dependency_directories() {
    let dir = TempDir::new().unwrap();
    let mgr = DefaultCacheManager::with_project_root(dir.path().to_path_buf());
    mgr.cache_proto(
        "remove",
        &[ProtoFile {
            name: "service.proto".into(),
            path: PathBuf::new(),
            content: "// proto".into(),
            services: vec![],
        }],
    )
    .await
    .unwrap();

    mgr.retain_aliases(&std::collections::HashSet::new())
        .await
        .unwrap();
    assert!(!dir.path().join("protos/remote/remove").exists());
}
