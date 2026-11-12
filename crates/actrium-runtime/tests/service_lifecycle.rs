use std::{fs, time::Duration};

use actrium_runtime::*;
use serde_json::{Value, json};
use tempfile::TempDir;

fn program(version: &str) -> ProgramRelease {
    ProgramRelease {
        publisher: "actrium".into(),
        name: "documents".into(),
        version: version.into(),
    }
}

fn workspace() -> WorkspaceBinding {
    WorkspaceBinding {
        workspace_id: "ws_team_notes".into(),
        display_name: "Team Notes".into(),
        provider: "local-folder".into(),
        locator: "opaque:team-notes".into(),
        access_mode: "read-write".into(),
        credential_ref: Some("keychain://team-notes".into()),
        lease_policy: "exclusive-writer".into(),
        created_at_epoch_secs: 1_700_000_000,
    }
}

fn request(key: &str) -> CreateInstanceRequest {
    CreateInstanceRequest {
        idempotency_key: key.into(),
        package_sources: vec!["signed-catalog://actrium/documents/1.0.0".into()],
        program_release: program("1.0.0"),
        service_package_digest: "sha256:service-v1".into(),
        front_package_digest: Some("sha256:front-v1".into()),
        privacy_contract_digest: Some("sha256:privacy-v1".into()),
        realm_name: "notes@acme.com".into(),
        workspace: Some(workspace()),
        metadata: Some(json!({"icon": "notes", "favorite": true})),
        requested_permissions: vec!["documents.read".into(), "documents.write".into()],
    }
}

fn attach_request(actor_id: &str) -> (AttachRequest, VerifiedChannel) {
    let request = AttachRequest {
        attach_id: "attach-1".into(),
        front_actor_id: "front.notes@acme.com".into(),
        service_actor_id: actor_id.into(),
        channel_id: "channel-1".into(),
        channel_transcript_digest: vec![1; 32],
        program: program("1.0.0"),
        package_digest: vec![2; 32],
        guest_digest: vec![3; 32],
        runtime: Runtime {
            actrium_version: "0.11".into(),
            front_host_api: "1".into(),
            keel_api: "1".into(),
        },
        protocol_fingerprint: vec![4; 32],
        tab_session_nonce: vec![5; 32],
        requested_lease: LeaseMode::Exclusive,
        requested_capabilities: Vec::new(),
        challenge_response: Vec::new(),
    };
    let channel = VerifiedChannel {
        channel_id: "channel-1".into(),
        channel_transcript_digest: vec![1; 32],
        peer_actor_id: "front.notes@acme.com".into(),
        peer_actor_type: "actrium.front".into(),
        peer_member: "member".into(),
        peer_client_key: "client-key".into(),
        peer_role: MemberRole::Owner,
        peer_actor_public_key: vec![6; 32],
        peer_credential_expires_at_epoch_secs: u64::MAX,
        local_runtime_instance_id: "runtime".into(),
        peer_runtime_instance_id: "front-runtime".into(),
        negotiation_digest: vec![7; 32],
    };
    (request, channel)
}

async fn create(service: &LifecycleService, key: &str) -> ServiceInstance {
    service.create(request(key)).await.unwrap()
}

fn set_persisted_state(service: &LifecycleService, actor_id: &str, state: LifecycleState) {
    let path = service.instance_dir(actor_id).join("instance.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value["lifecycle_state"] = serde_json::to_value(state).unwrap();
    fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

#[test]
fn lifecycle_transition_table_is_complete_and_rejects_every_illegal_edge() {
    use LifecycleState::*;
    let states = [
        Creating, Stopped, Starting, Running, Suspended, Stopping, Migrating, Archived, Trashed,
        Deleting, Deleted,
    ];
    let allowed = [
        (Creating, Stopped),
        (Stopped, Starting),
        (Starting, Running),
        (Starting, Stopped),
        (Running, Suspended),
        (Suspended, Running),
        (Running, Stopping),
        (Suspended, Stopping),
        (Stopping, Stopped),
        (Stopped, Migrating),
        (Running, Migrating),
        (Migrating, Stopped),
        (Stopped, Archived),
        (Archived, Stopped),
        (Stopped, Trashed),
        (Archived, Trashed),
        (Trashed, Stopped),
        (Trashed, Deleting),
        (Deleting, Deleted),
    ];
    for from in states {
        for to in states {
            assert_eq!(
                from.can_transition_to(to),
                allowed.contains(&(from, to)),
                "unexpected transition result for {from:?} -> {to:?}"
            );
        }
    }
}

#[tokio::test]
async fn lifecycle_operations_keep_inventory_fields_independent() {
    let temp = TempDir::new().unwrap();
    let service = LifecycleService::open(temp.path()).unwrap();
    let created = create(&service, "state-machine").await;
    let actor_id = created.actor_id.clone();

    assert_eq!(created.lifecycle_state, LifecycleState::Stopped);
    assert_eq!(created.desired_state, DesiredState::Stopped);
    assert_eq!(created.reachability, Reachability::Offline);
    assert_eq!(created.health, Health::Unknown);
    assert_eq!(created.attachment, AttachmentState::Idle);
    assert_eq!(created.placement, Placement::Local);
    assert_eq!(created.workspace, Some(workspace()));
    assert_eq!(created.metadata, request("state-machine").metadata);
    assert!(!service.attach_allowed(&actor_id).unwrap());

    let error = service
        .start_with(&actor_id, || {
            Err(LifecycleError::Injected("workload did not start".into()))
        })
        .await
        .unwrap_err();
    assert!(matches!(error, LifecycleError::Injected(_)));
    let failed = service.instance(&actor_id).unwrap();
    assert_eq!(failed.lifecycle_state, LifecycleState::Stopped);
    assert_eq!(failed.health, Health::Failed);
    assert_eq!(failed.desired_state, DesiredState::Running);
    assert_eq!(failed.reachability, Reachability::Offline);

    let running = service.start(&actor_id).await.unwrap();
    assert_eq!(running.lifecycle_state, LifecycleState::Running);
    assert_eq!(running.health, Health::Healthy);
    assert_eq!(running.reachability, Reachability::Online);
    assert!(service.attach_allowed(&actor_id).unwrap());

    let illegal = service.archive(&actor_id).await.unwrap_err();
    assert!(matches!(
        illegal,
        LifecycleError::InvalidTransition {
            from: LifecycleState::Running,
            to: LifecycleState::Archived
        }
    ));

    let suspended = service.suspend(&actor_id).await.unwrap();
    assert_eq!(suspended.lifecycle_state, LifecycleState::Suspended);
    assert_eq!(suspended.desired_state, DesiredState::Running);
    assert!(!service.attach_allowed(&actor_id).unwrap());
    let (attach, channel) = attach_request(&actor_id);
    assert!(matches!(
        service.attach(attach, &channel).await.err().unwrap(),
        LifecycleError::NotAttachable {
            state: LifecycleState::Suspended,
            ..
        }
    ));
    assert!(service.start(&actor_id).await.is_err());

    let resumed = service.resume(&actor_id).await.unwrap();
    assert_eq!(resumed.actor_id, actor_id);
    assert!(service.attach_allowed(&actor_id).unwrap());
    let stopped = service.stop(&actor_id).await.unwrap();
    assert_eq!(stopped.lifecycle_state, LifecycleState::Stopped);
    assert!(!service.attach_allowed(&actor_id).unwrap());

    let archived = service.archive(&actor_id).await.unwrap();
    assert_eq!(archived.desired_state, DesiredState::Archived);
    assert_eq!(archived.lifecycle_state, LifecycleState::Archived);
    let restored = service.restore(&actor_id).await.unwrap();
    assert_eq!(restored.lifecycle_state, LifecycleState::Stopped);

    let trashed = service.trash(&actor_id).await.unwrap();
    assert_eq!(trashed.lifecycle_state, LifecycleState::Trashed);
    assert!(!service.attach_allowed(&actor_id).unwrap());
    let restored = service.restore(&actor_id).await.unwrap();
    assert_eq!(restored.lifecycle_state, LifecycleState::Stopped);

    let migrated = service.migrate(&actor_id, Placement::Remote).await.unwrap();
    assert_eq!(migrated.lifecycle_state, LifecycleState::Stopped);
    assert_eq!(migrated.placement, Placement::Remote);
    assert_eq!(migrated.runtime_location, "remote");
    assert_eq!(migrated.actor_id, actor_id);
    assert_eq!(migrated.realm_name, "notes@acme.com");
}

#[tokio::test]
async fn lifecycle_creation_is_idempotent_and_uses_hashed_atomic_storage() {
    let temp = TempDir::new().unwrap();
    let service = LifecycleService::open(temp.path()).unwrap();
    let first = create(&service, "same-request").await;
    let second = service.create(request("same-request")).await.unwrap();

    assert_eq!(first.actor_id, second.actor_id);
    assert_eq!(first.runtime_instance_id, second.runtime_instance_id);
    assert_eq!(first.actor_id_hash, actor_id_hash(&first.actor_id));
    assert_eq!(first.actor_id_hash.len(), 64);
    assert!(first.actor_id.ends_with(".notes@acme.com"));
    assert_eq!(
        service.instance_dir(&first.actor_id).file_name().unwrap(),
        first.actor_id_hash.as_str()
    );

    let instance_path = service.instance_dir(&first.actor_id).join("instance.json");
    let persisted: Value = serde_json::from_slice(&fs::read(instance_path).unwrap()).unwrap();
    assert_eq!(persisted["actor_id"], first.actor_id);
    assert_eq!(persisted["store_path"], "actor-store");
    assert!(service.store_dir(&first.actor_id).is_dir());
    assert!(service.tombstone(&first.actor_id).unwrap().is_none());
    assert!(
        service
            .instance_dir(&first.actor_id)
            .join("workload-state")
            .is_dir()
    );
    assert!(
        service
            .instance_dir(&first.actor_id)
            .join("snapshots")
            .is_dir()
    );
    assert!(service.instance_dir(&first.actor_id).join("logs").is_dir());

    for entry in fs::read_dir(service.instance_dir(&first.actor_id)).unwrap() {
        let name = entry.unwrap().file_name();
        assert!(!name.to_string_lossy().ends_with(".tmp"));
    }
    let intent = service.creation_intent("same-request").unwrap().unwrap();
    assert_eq!(intent.stage, CreationStage::Completed);
    assert_eq!(
        intent.result_actor_id.as_deref(),
        Some(first.actor_id.as_str())
    );

    let mut conflicting = request("same-request");
    conflicting.service_package_digest = "sha256:different".into();
    assert!(matches!(
        service.create(conflicting).await.unwrap_err(),
        LifecycleError::IdempotencyConflict
    ));
    assert_eq!(
        fs::read_dir(temp.path().join("services")).unwrap().count(),
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lifecycle_concurrent_managers_cannot_allocate_two_identities_for_one_key() {
    let temp = TempDir::new().unwrap();
    let first_manager = LifecycleService::open(temp.path()).unwrap();
    let second_manager = LifecycleService::open(temp.path()).unwrap();
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let mut entered = Some(entered_tx);

    let first = tokio::spawn(async move {
        first_manager
            .create_with_observer(request("concurrent-create"), move |stage| {
                if stage == CreationStage::Prepared {
                    entered.take().unwrap().send(()).unwrap();
                    std::thread::sleep(Duration::from_millis(100));
                }
                Ok(())
            })
            .await
            .unwrap()
    });
    entered_rx.await.unwrap();
    assert!(matches!(
        second_manager
            .create(request("concurrent-create"))
            .await
            .unwrap_err(),
        LifecycleError::ConcurrentCreation(_)
    ));
    let created = first.await.unwrap();
    let retried = second_manager
        .create(request("concurrent-create"))
        .await
        .unwrap();
    assert_eq!(retried.actor_id, created.actor_id);
    assert_eq!(
        fs::read_dir(temp.path().join("services")).unwrap().count(),
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lifecycle_creation_recovers_after_every_durable_stage() {
    let stages = [
        CreationStage::Prepared,
        CreationStage::PackageVerified,
        CreationStage::UserApproved,
        CreationStage::IdentityAllocated,
        CreationStage::StorageCommitted,
        CreationStage::ServiceStarted,
        CreationStage::FrontAttached,
        CreationStage::Completed,
    ];

    for crash_at in stages {
        let temp = TempDir::new().unwrap();
        let service = LifecycleService::open(temp.path()).unwrap();
        let error = service
            .create_with_observer(request("crash-saga"), |stage| {
                if stage == crash_at {
                    Err(LifecycleError::Injected(format!("crash after {stage:?}")))
                } else {
                    Ok(())
                }
            })
            .await
            .unwrap_err();
        assert!(matches!(error, LifecycleError::Injected(_)));
        let interrupted = service.creation_intent("crash-saga").unwrap().unwrap();
        assert_eq!(interrupted.stage, crash_at);
        let allocated = interrupted.result_actor_id.clone();
        if crash_at >= CreationStage::IdentityAllocated {
            assert!(allocated.is_some(), "identity missing after {crash_at:?}");
        }
        drop(service);

        let reopened = LifecycleService::open(temp.path()).unwrap();
        let recovered = reopened.create(request("crash-saga")).await.unwrap();
        if let Some(allocated) = allocated {
            assert_eq!(
                recovered.actor_id, allocated,
                "ActorId changed after {crash_at:?}"
            );
        }
        assert_eq!(recovered.lifecycle_state, LifecycleState::Stopped);
        assert!(reopened.store_dir(&recovered.actor_id).is_dir());
        assert!(reopened.tombstone(&recovered.actor_id).unwrap().is_none());
        assert_eq!(
            reopened
                .creation_intent("crash-saga")
                .unwrap()
                .unwrap()
                .stage,
            CreationStage::Completed
        );
    }
}

#[tokio::test]
async fn lifecycle_stop_restart_trash_and_purge_rotate_runtime_but_not_actor_id() {
    let temp = TempDir::new().unwrap();
    let service = LifecycleService::open(temp.path()).unwrap();
    let created = create(&service, "identity-rotation").await;
    let actor_id = created.actor_id.clone();

    service.start(&actor_id).await.unwrap();
    let restarted = service.restart(&actor_id).await.unwrap();
    assert_eq!(restarted.actor_id, actor_id);
    assert_ne!(restarted.runtime_instance_id, created.runtime_instance_id);

    let stopped = service.stop(&actor_id).await.unwrap();
    assert_eq!(stopped.actor_id, actor_id);
    assert_ne!(stopped.runtime_instance_id, restarted.runtime_instance_id);

    service.start(&actor_id).await.unwrap();
    service.suspend(&actor_id).await.unwrap();
    let stopped_from_suspend = service.stop(&actor_id).await.unwrap();
    assert_ne!(
        stopped_from_suspend.runtime_instance_id,
        stopped.runtime_instance_id
    );

    let trashed = service.trash(&actor_id).await.unwrap();
    assert_eq!(trashed.actor_id, actor_id);
    assert_ne!(
        trashed.runtime_instance_id,
        stopped_from_suspend.runtime_instance_id
    );
    let restored = service.restore(&actor_id).await.unwrap();
    assert_eq!(restored.actor_id, actor_id);
    assert_eq!(restored.runtime_instance_id, trashed.runtime_instance_id);

    service.trash(&actor_id).await.unwrap();
    let before_purge = service.instance(&actor_id).unwrap();
    let tombstone = service.purge(&actor_id).await.unwrap();
    assert_eq!(tombstone.actor_id, actor_id);
    assert_ne!(
        tombstone.runtime_instance_id,
        before_purge.runtime_instance_id
    );
    assert_eq!(tombstone.lifecycle_state, LifecycleState::Deleted);
    assert!(!tombstone.recoverable);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lifecycle_concurrent_start_has_exactly_one_winner() {
    let temp = TempDir::new().unwrap();
    let service = LifecycleService::open(temp.path()).unwrap();
    let created = create(&service, "concurrent-start").await;
    let actor_id = created.actor_id;
    let other = service.clone();

    let first = service.start(&actor_id);
    let second = other.start(&actor_id);
    let (first, second) = tokio::join!(first, second);
    let successes = [&first, &second]
        .iter()
        .filter(|result| result.is_ok())
        .count();
    assert_eq!(successes, 1, "first={first:?}, second={second:?}");
    let failure = if first.is_err() { first } else { second }.unwrap_err();
    assert!(matches!(
        failure,
        LifecycleError::InvalidTransition {
            from: LifecycleState::Running,
            to: LifecycleState::Starting
        }
    ));
    assert_eq!(
        service.instance(&actor_id).unwrap().lifecycle_state,
        LifecycleState::Running
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lifecycle_start_lock_excludes_a_second_manager_on_the_same_directory() {
    let temp = TempDir::new().unwrap();
    let first_manager = LifecycleService::open(temp.path()).unwrap();
    let second_manager = LifecycleService::open(temp.path()).unwrap();
    let created = create(&first_manager, "cross-manager-start").await;
    let actor_id = created.actor_id;
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();

    let first = tokio::spawn({
        let manager = first_manager.clone();
        let actor_id = actor_id.clone();
        async move {
            manager
                .start_with(&actor_id, || {
                    entered_tx.send(()).unwrap();
                    std::thread::sleep(Duration::from_millis(100));
                    Ok(())
                })
                .await
        }
    });
    entered_rx.await.unwrap();
    let error = second_manager.start(&actor_id).await.unwrap_err();
    assert!(matches!(error, LifecycleError::ConcurrentStart(_)));
    first.await.unwrap().unwrap();
    assert_eq!(
        first_manager.instance(&actor_id).unwrap().lifecycle_state,
        LifecycleState::Running
    );
    assert!(
        !first_manager
            .instance_dir(&actor_id)
            .join("start.lock")
            .exists()
    );
}

#[tokio::test]
async fn lifecycle_trash_retains_private_state_and_restore_reuses_identity() {
    let temp = TempDir::new().unwrap();
    let service = LifecycleService::open(temp.path()).unwrap();
    let created = create(&service, "trash-restore").await;
    let actor_id = created.actor_id;
    let store_file = service.store_dir(&actor_id).join("document.json");
    fs::write(&store_file, b"important private state").unwrap();

    service.trash(&actor_id).await.unwrap();
    assert_eq!(fs::read(&store_file).unwrap(), b"important private state");
    assert_eq!(
        service.instance(&actor_id).unwrap().workspace,
        Some(workspace())
    );

    let restored = service.restore(&actor_id).await.unwrap();
    assert_eq!(restored.actor_id, actor_id);
    assert_eq!(restored.lifecycle_state, LifecycleState::Stopped);
    assert_eq!(fs::read(&store_file).unwrap(), b"important private state");

    service.archive(&actor_id).await.unwrap();
    service.trash(&actor_id).await.unwrap();
    assert_eq!(service.instance(&actor_id).unwrap().actor_id, actor_id);
}

#[tokio::test]
async fn lifecycle_purge_is_permanent() {
    let temp = TempDir::new().unwrap();
    let service = LifecycleService::open(temp.path()).unwrap();
    let created = create(&service, "permanent-purge").await;
    let actor_id = created.actor_id;
    let store_file = service.store_dir(&actor_id).join("secret.json");
    fs::write(&store_file, b"private").unwrap();
    let workspace_dir = temp.path().join("workspaces").join("ws_team_notes");
    fs::create_dir_all(&workspace_dir).unwrap();
    fs::write(workspace_dir.join("note.txt"), b"shared workspace").unwrap();

    service.trash(&actor_id).await.unwrap();
    let tombstone = service.purge(&actor_id).await.unwrap();
    assert_eq!(tombstone.reason, TombstoneReason::Purged);
    assert_eq!(tombstone.credential_state, CredentialState::Revoked);
    assert_eq!(tombstone.lifecycle_state, LifecycleState::Deleted);
    assert!(!tombstone.recoverable);
    assert!(!service.store_dir(&actor_id).exists());
    assert!(
        !service
            .instance_dir(&actor_id)
            .join("instance.json")
            .exists()
    );

    assert_eq!(
        fs::read(workspace_dir.join("note.txt")).unwrap(),
        b"shared workspace"
    );

    assert!(matches!(
        service.instance(&actor_id),
        Err(LifecycleError::Deleted(_))
    ));
    assert!(matches!(
        service.restore(&actor_id).await.unwrap_err(),
        LifecycleError::Deleted(_)
    ));
    assert!(matches!(
        service.start(&actor_id).await.unwrap_err(),
        LifecycleError::Deleted(_)
    ));
    assert!(matches!(
        service
            .create(request("permanent-purge"))
            .await
            .unwrap_err(),
        LifecycleError::Deleted(_)
    ));
    let persisted = service.tombstone(&actor_id).unwrap().unwrap();
    assert_eq!(persisted.actor_id, actor_id);
    assert_eq!(persisted.reason, TombstoneReason::Purged);
}

#[tokio::test]
async fn lifecycle_upgrade_archives_old_identity_and_creates_new_actor() {
    let temp = TempDir::new().unwrap();
    let service = LifecycleService::open(temp.path()).unwrap();
    let old = create(&service, "upgrade-old").await;
    service.start(&old.actor_id).await.unwrap();
    fs::write(
        service.store_dir(&old.actor_id).join("v1-state"),
        b"retained",
    )
    .unwrap();

    let mut replacement = request("upgrade-new");
    replacement.program_release = program("2.0.0");
    replacement.service_package_digest = "sha256:service-v2".into();
    replacement.workspace = None;
    replacement.metadata = None;
    let outcome = service.upgrade(&old.actor_id, replacement).await.unwrap();

    assert_eq!(outcome.previous.actor_id, old.actor_id);
    assert_eq!(outcome.previous.lifecycle_state, LifecycleState::Archived);
    assert_ne!(outcome.upgraded.actor_id, old.actor_id);
    assert_eq!(outcome.upgraded.realm_name, old.realm_name);
    assert_eq!(outcome.upgraded.program_release.version, "2.0.0");
    assert_eq!(outcome.upgraded.workspace, old.workspace);
    assert_eq!(outcome.upgraded.metadata, old.metadata);
    assert_eq!(outcome.upgraded.lifecycle_state, LifecycleState::Stopped);
    assert!(service.store_dir(&old.actor_id).join("v1-state").exists());

    let mut wrong_realm = request("wrong-realm");
    wrong_realm.program_release = program("3.0.0");
    wrong_realm.realm_name = "notes@other.example".into();
    assert!(matches!(
        service
            .upgrade(&old.actor_id, wrong_realm)
            .await
            .unwrap_err(),
        LifecycleError::InvalidRequest(_)
    ));
}

#[tokio::test]
async fn lifecycle_process_recovery_converges_transient_and_online_states() {
    let temp = TempDir::new().unwrap();
    let service = LifecycleService::open(temp.path()).unwrap();
    let created = create(&service, "process-recovery").await;
    let actor_id = created.actor_id;
    let running = service.start(&actor_id).await.unwrap();
    drop(service);

    let reopened = LifecycleService::open(temp.path()).unwrap();
    let stale_creation_lock = temp
        .path()
        .join("creation-intents")
        .join(format!("{}.lock", actor_id_hash("crashed-create")));
    fs::write(&stale_creation_lock, b"crashed process").unwrap();
    reopened.recover().await.unwrap();
    assert!(!stale_creation_lock.exists());
    let stopped = reopened.instance(&actor_id).unwrap();
    assert_eq!(stopped.lifecycle_state, LifecycleState::Stopped);
    assert_eq!(stopped.reachability, Reachability::Offline);
    assert_ne!(stopped.runtime_instance_id, running.runtime_instance_id);

    set_persisted_state(&reopened, &actor_id, LifecycleState::Starting);
    fs::write(
        reopened.instance_dir(&actor_id).join("start.lock"),
        b"crashed process",
    )
    .unwrap();
    let before = reopened.instance(&actor_id).unwrap().runtime_instance_id;
    reopened.recover().await.unwrap();
    assert!(!reopened.instance_dir(&actor_id).join("start.lock").exists());
    let failed = reopened.instance(&actor_id).unwrap();
    assert_eq!(failed.lifecycle_state, LifecycleState::Stopped);
    assert_eq!(failed.health, Health::Failed);
    assert_ne!(failed.runtime_instance_id, before);

    for state in [LifecycleState::Stopping, LifecycleState::Migrating] {
        set_persisted_state(&reopened, &actor_id, state);
        let before = reopened.instance(&actor_id).unwrap().runtime_instance_id;
        reopened.recover().await.unwrap();
        let recovered = reopened.instance(&actor_id).unwrap();
        assert_eq!(recovered.lifecycle_state, LifecycleState::Stopped);
        assert_eq!(recovered.health, Health::Unknown);
        assert_ne!(recovered.runtime_instance_id, before);
    }

    set_persisted_state(&reopened, &actor_id, LifecycleState::Deleting);
    let before = reopened.instance(&actor_id).unwrap().runtime_instance_id;
    reopened.recover().await.unwrap();
    let tombstone = reopened.tombstone(&actor_id).unwrap().unwrap();
    assert_eq!(tombstone.lifecycle_state, LifecycleState::Deleted);
    assert_ne!(tombstone.runtime_instance_id, before);
    assert!(!reopened.store_dir(&actor_id).exists());
}

#[tokio::test]
async fn lifecycle_files_survive_reopen_without_changing_identity() {
    let temp = TempDir::new().unwrap();
    let service = LifecycleService::open(temp.path()).unwrap();
    let created = create(&service, "reopen").await;
    let actor_id = created.actor_id.clone();
    service.start(&actor_id).await.unwrap();
    service.suspend(&actor_id).await.unwrap();
    let suspended = service.instance(&actor_id).unwrap();
    drop(service);
    tokio::time::sleep(Duration::from_millis(1)).await;

    let reopened = LifecycleService::open(temp.path()).unwrap();
    let loaded = reopened.instance(&actor_id).unwrap();
    assert_eq!(loaded.actor_id, created.actor_id);
    assert_eq!(loaded.runtime_instance_id, suspended.runtime_instance_id);
    assert_eq!(loaded.lifecycle_state, LifecycleState::Suspended);
    assert!(!reopened.attach_allowed(&actor_id).unwrap());
    reopened.recover().await.unwrap();
    assert_eq!(reopened.instance(&actor_id).unwrap().actor_id, actor_id);
}
