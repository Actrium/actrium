#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

if ! command -v rg >/dev/null 2>&1; then
  echo "error: ripgrep (rg) is required for Rust boundary hygiene checks" >&2
  exit 127
fi

failures=0

record_failure() {
  failures=$((failures + 1))
}

print_ok() {
  echo "[ok] $1"
}

print_failure() {
  echo
  echo "[fail] $1"
  record_failure
}

browser_sync_product_direction_requirements=(
  "docs/architecture/browser-sync-cloud-drive.md|Actro should not operate a browser sync service|Browser sync architecture rejects an Actro-operated product sync service"
  "docs/architecture/browser-sync-cloud-drive.md|must not embed a sync server|Browser sync architecture rejects an embedded browser sync server"
  "docs/architecture/browser-sync-cloud-drive.md|Provider Model|Browser sync architecture defines a cloud provider boundary"
  "docs/architecture/browser-sync-cloud-drive.md|BrowserStorageSyncCloudStore|Browser sync architecture names the Rust cloud store trait boundary"
  "docs/architecture/browser-sync-cloud-drive.md|cloud_store_contract_json|Browser sync architecture names the cloud store binding contract"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_provider_connection_status_policy_json|Browser sync architecture names the Rust provider connection status policy"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_provider_error_policy_json|Browser sync architecture names the Rust provider error policy"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_provider_error_classification_json|Browser sync architecture names the Rust provider error classifier"
  "docs/architecture/browser-sync-cloud-drive.md|providerErrorClassification|Browser sync architecture records provider error diagnostics source mapping"
  "docs/architecture/browser-sync-cloud-drive.md|nested round/client result shapes|Browser sync architecture records nested provider round error extraction"
  "docs/architecture/browser-sync-cloud-drive.md|localFolderProviderRoundErrors|Browser sync architecture records the local-folder provider round error source"
  "docs/architecture/rust-core.md|nested round/client result shapes|Rust core doc records nested provider round error extraction"
  "docs/architecture/rust-core.md|localFolderProviderRoundErrors|Rust core doc records the local-folder provider round error source"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_store_capability_preflight_json|Browser sync architecture names the Rust cloud store capability preflight"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_push_plan_json|Browser sync architecture names the Rust cloud push write planner"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_push_result_plan_json|Browser sync architecture names the Rust cloud push result planner"
  "docs/architecture/browser-sync-cloud-drive.md|writeSummary|Browser sync architecture records cloud push write summaries"
  "docs/architecture/browser-sync-cloud-drive.md|ackReason|Browser sync architecture records cloud push ack reasons"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_pull_plan_json|Browser sync architecture names the Rust cloud pull read planner"
  "docs/architecture/browser-sync-cloud-drive.md|readSummary|Browser sync architecture records cloud pull read summaries"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_pull_apply_plan_json|Browser sync architecture names the Rust cloud pull apply planner"
  "docs/architecture/browser-sync-cloud-drive.md|skippedSnapshots|Browser sync architecture records cloud pull apply skipped-snapshot diagnostics"
  "docs/architecture/browser-sync-cloud-drive.md|skippedLogs|Browser sync architecture records cloud pull apply skipped-log diagnostics"
  "docs/architecture/browser-sync-cloud-drive.md|applyAction|Browser sync architecture records cloud pull apply action diagnostics"
  "docs/architecture/browser-sync-cloud-drive.md|applySummary|Browser sync architecture records cloud pull apply summaries"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_pull_continuation_plan_json|Browser sync architecture names the Rust cloud pull continuation planner"
  "docs/architecture/browser-sync-cloud-drive.md|continuationSummary|Browser sync architecture records cloud pull continuation summaries"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_execute_planned_reads_with_continuations|Browser sync architecture records Rust-backed pull pagination execution"
  "docs/architecture/browser-sync-cloud-drive.md|paginationLimitExceeded|Browser sync architecture records pull pagination safety diagnostics"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_compatibility_status_json|Browser sync architecture names the Rust cloud compatibility gate"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_compatibility_preflight_json|Browser sync architecture names the Rust batched cloud compatibility preflight"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_schema_rollout_plan_json|Browser sync architecture names the Rust cloud schema rollout plan"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_schema_migration_playbook_json|Browser sync architecture names the Rust cloud schema migration playbook"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_recovery_snapshot_plan_json|Browser sync architecture names the Rust cloud recovery snapshot planner"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_recovery_snapshot_result_plan_json|Browser sync architecture names the Rust cloud recovery snapshot result handoff"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_local_folder_recovery_snapshot_round_json|Browser sync architecture names the Rust local-folder recovery snapshot round"
  "docs/architecture/browser-sync-cloud-drive.md|BrowserStorageSyncCloudRecoverySnapshotRoundSummary|Browser sync architecture names the Rust cloud recovery snapshot round summary"
  "docs/architecture/browser-sync-cloud-drive.md|writeRecoveryCopy|Browser sync architecture records cloud recovery snapshot copy operations"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_membership_plan_json|Browser sync architecture names the Rust cloud device membership planner"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_membership_result_plan_json|Browser sync architecture names the Rust cloud device membership result planner"
  "docs/architecture/browser-sync-cloud-drive.md|writeReason|Browser sync architecture records cloud planned write reasons"
  "docs/architecture/browser-sync-cloud-drive.md|readReason|Browser sync architecture records cloud planned read reasons"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_compaction_plan_json|Browser sync architecture names the Rust cloud compaction planner"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_compaction_result_plan_json|Browser sync architecture names the Rust cloud compaction result planner"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_local_folder_compaction_round_json|Browser sync architecture names the Rust local-folder compaction round"
  "docs/architecture/browser-sync-cloud-drive.md|BrowserStorageSyncCloudCompactionRoundSummary|Browser sync architecture names the Rust cloud compaction round summary"
  "docs/architecture/browser-sync-cloud-drive.md|deleteReason|Browser sync architecture records cloud planned delete reasons"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_high_watermark_policy_json|Browser sync architecture names the Rust cloud high-watermark policy"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_high_watermark_plan_json|Browser sync architecture names the Rust cloud high-watermark retention planner"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_cleanup_result_plan_json|Browser sync architecture names the Rust cloud cleanup result planner"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_local_folder_cleanup_round_json|Browser sync architecture names the Rust local-folder cleanup round"
  "docs/architecture/browser-sync-cloud-drive.md|BrowserStorageSyncCloudCleanupRoundSummary|Browser sync architecture names the Rust cloud cleanup round summary"
  "docs/architecture/browser-sync-cloud-drive.md|BrowserStorageSyncMemoryCloudStore|Browser sync architecture names the Rust in-memory cloud store fixture"
  "docs/architecture/browser-sync-cloud-drive.md|BrowserStorageSyncLocalFolderCloudStore|Browser sync architecture names the Rust local-folder cloud store fixture"
  "docs/architecture/browser-sync-cloud-drive.md|not a per-operation FFI surface|Browser sync architecture keeps local-folder store coarse-grained"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_local_folder_capabilities_json|Browser sync architecture names the Rust local-folder capability helper"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_local_folder_capability_preflight_json|Browser sync architecture names the Rust local-folder capability preflight helper"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_local_folder_compatibility_preflight_json|Browser sync architecture names the Rust local-folder compatibility preflight helper"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_local_folder_cycle_round_json|Browser sync architecture names the Rust local-folder cycle JSON entry"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_local_folder_membership_round_json|Browser sync architecture names the Rust local-folder membership JSON entry"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_execute_planned_writes|Browser sync architecture names the Rust cloud planned write executor"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_run_membership_round|Browser sync architecture names the Rust cloud membership round runner"
  "docs/architecture/browser-sync-cloud-drive.md|BrowserStorageSyncCloudMembershipRoundSummary|Browser sync architecture names the Rust cloud membership round summary"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_run_push_round|Browser sync architecture names the Rust cloud push round runner"
  "docs/architecture/browser-sync-cloud-drive.md|BrowserStorageSyncCloudPushRoundSummary|Browser sync architecture names the Rust cloud push round summary"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_run_pull_round|Browser sync architecture names the Rust cloud pull round runner"
  "docs/architecture/browser-sync-cloud-drive.md|BrowserStorageSyncCloudPullRoundSummary|Browser sync architecture names the Rust cloud pull round summary"
  "docs/architecture/browser-sync-cloud-drive.md|roundSummary|Browser sync architecture records cloud pull round summaries"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_run_cycle_round|Browser sync architecture names the Rust cloud cycle round runner"
  "docs/architecture/browser-sync-cloud-drive.md|BrowserStorageSyncCloudCycleRoundSummary|Browser sync architecture names the Rust cloud cycle round summary"
  "docs/architecture/browser-sync-cloud-drive.md|client-step categories|Browser sync architecture records cloud cycle round summaries"
  "docs/architecture/browser-sync-cloud-drive.md|BrowserStorageSyncCloudCycleClientPlan|Browser sync architecture names the Rust cloud cycle client plan"
  "docs/architecture/browser-sync-cloud-drive.md|stepReason|Browser sync architecture records cloud cycle client step reasons"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_cycle_status_policy_json|Browser sync architecture names the Rust cloud cycle status policy"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_membership_status_policy_json|Browser sync architecture names the Rust cloud membership status policy"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_cycle_schedule_state_json|Browser sync architecture names the Rust cloud cycle schedule state"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_membership_schedule_state_json|Browser sync architecture names the Rust cloud membership schedule state"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_membership_attempt_plan_json|Browser sync architecture names the Rust cloud membership attempt plan"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_membership_loop_plan_json|Browser sync architecture names the Rust cloud membership loop plan"
  "docs/architecture/browser-sync-cloud-drive.md|BrowserStorageSyncCloudMembershipLoopSummary|Browser sync architecture names the Rust cloud membership loop summary"
  "docs/architecture/browser-sync-cloud-drive.md|membership \`loopSummary\`|Browser sync architecture records cloud membership loop summaries"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_provider_diagnostics_json|Browser sync architecture names the Rust cloud provider diagnostics summary"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_observability_event_json|Browser sync architecture names the Rust cloud observability event contract"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_cycle_attempt_plan_json|Browser sync architecture names the Rust cloud cycle attempt plan"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_sync_cloud_cycle_loop_plan_json|Browser sync architecture names the Rust cloud cycle loop plan"
  "docs/architecture/browser-sync-cloud-drive.md|BrowserStorageSyncCloudCycleLoopSummary|Browser sync architecture names the Rust cloud cycle loop summary"
  "docs/architecture/browser-sync-cloud-drive.md|cycle \`loopSummary\`|Browser sync architecture records cloud cycle loop summaries"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_address_canonicalization_policy_json|Browser sync architecture names the Rust address canonicalization policy"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_canonicalize_addresses_json|Browser sync architecture names the Rust batched address canonicalizer"
  "docs/architecture/browser-sync-cloud-drive.md|credentialStripped|Browser sync architecture records Rust-owned URL credential stripping diagnostics"
  "docs/architecture/browser-sync-cloud-drive.md|canonicalizationSummary|Browser sync architecture records address canonicalization summaries"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_bookmark_folder_tree_integrity_policy_json|Browser sync architecture names the Rust bookmark folder tree integrity policy"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_bookmark_folder_tree_integrity_plan_json|Browser sync architecture names the Rust bookmark folder tree integrity planner"
  "docs/architecture/browser-sync-cloud-drive.md|integrityAction|Browser sync architecture records bookmark folder integrity diagnostics"
  "docs/architecture/browser-sync-cloud-drive.md|integritySummary|Browser sync architecture records bookmark folder integrity summaries"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_bookmark_restore_placement_policy_json|Browser sync architecture names the Rust bookmark restore placement policy"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_bookmark_restore_placement_plan_json|Browser sync architecture names the Rust bookmark restore placement planner"
  "docs/architecture/browser-sync-cloud-drive.md|placementReason|Browser sync architecture records bookmark restore placement reasons"
  "docs/architecture/browser-sync-cloud-drive.md|stateConflictAction|Browser sync architecture records bookmark delete/restore conflict diagnostics"
  "docs/architecture/browser-sync-cloud-drive.md|stateConflictReason|Browser sync architecture records bookmark delete/restore conflict reasons"
  "docs/architecture/browser-sync-cloud-drive.md|stateConflicts|Browser sync architecture records bookmark delete/restore conflict details"
  "docs/architecture/browser-sync-cloud-drive.md|stateConflictSummary|Browser sync architecture records bookmark delete/restore conflict summaries"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_bookmark_ownership_policy_json|Browser sync architecture names the Rust bookmark ownership policy"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_bookmark_ownership_plan_json|Browser sync architecture names the Rust bookmark ownership planner"
  "docs/architecture/browser-sync-cloud-drive.md|ownershipSummary|Browser sync architecture records bookmark ownership summaries"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_history_visit_aggregation_policy_json|Browser sync architecture names the Rust history visit aggregation policy"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_history_visit_aggregation_plan_json|Browser sync architecture names the Rust history visit aggregation planner"
  "docs/architecture/browser-sync-cloud-drive.md|aggregationAction|Browser sync architecture records history visit aggregation diagnostics"
  "docs/architecture/browser-sync-cloud-drive.md|aggregationSummary|Browser sync architecture records history visit aggregation summaries"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_session_tab_ordering_policy_json|Browser sync architecture names the Rust session tab ordering policy"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_session_tab_ordering_plan_json|Browser sync architecture names the Rust session tab ordering planner"
  "docs/architecture/browser-sync-cloud-drive.md|orderingAction|Browser sync architecture records session tab ordering diagnostics"
  "docs/architecture/browser-sync-cloud-drive.md|orderingSummary|Browser sync architecture records session tab ordering summaries"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_session_active_tab_consistency_policy_json|Browser sync architecture names the Rust session active-tab consistency policy"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_session_active_tab_consistency_plan_json|Browser sync architecture names the Rust session active-tab consistency planner"
  "docs/architecture/browser-sync-cloud-drive.md|consistencyReason|Browser sync architecture records session active-tab consistency diagnostics"
  "docs/architecture/browser-sync-cloud-drive.md|consistencySummary|Browser sync architecture records session active-tab consistency summaries"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_retention_pruning_policy_json|Browser sync architecture names the Rust retention pruning policy"
  "docs/architecture/browser-sync-cloud-drive.md|browser_storage_retention_pruning_plan_json|Browser sync architecture names the Rust retention pruning planner"
  "docs/architecture/browser-sync-cloud-drive.md|retentionReason|Browser sync architecture records retention pruning diagnostics"
  "docs/architecture/browser-sync-cloud-drive.md|retentionSummary|Browser sync architecture records retention pruning summaries"
  "docs/architecture/browser-sync-cloud-drive.md|maxHistoryPagesPerOriginDevice|Browser sync architecture records Rust-owned per-origin-device history pruning"
  "docs/architecture/browser-sync-cloud-drive.md|maxSessionTabsPerOriginDevice|Browser sync architecture records Rust-owned per-origin-device session-tab pruning"
  "docs/architecture/browser-sync-cloud-drive.md|manifest.json|Browser sync architecture defines cloud manifest/log storage"
  "docs/architecture/browser-sync-cloud-drive.md|No Actro product server allocates device identity|Browser sync architecture rejects server-side device registration"
  "docs/architecture/browser-sync-cloud-drive.md|Sharing a data structure and syncing files is necessary, but not sufficient|Browser sync architecture keeps deterministic sync rules in Rust"
  "docs/architecture/overview.md|browser-sync-cloud-drive.md|Architecture overview links the cloud-drive browser sync direction"
  "internal/pending-expansion.md|replacement memory for the quoted 2026-07-03 seven-point|Pending expansion tracks cloud-drive browser sync instead of an Actro-operated sync path"
  "internal/pending-expansion.md|provider-owned file sync|Pending expansion records provider-owned file sync as the replacement mainline"
  "internal/pending-expansion.md|Retired from the mainline|Pending expansion explicitly retires the old hosted-sync backlog"
  "internal/pending-expansion.md|not an Actro service API|Pending expansion rejects service-side device registration and revocation"
  "internal/pending-expansion.md|browser sync backend plus clients|Pending expansion rejects the old backend-plus-clients framing"
  "internal/pending-expansion.md|P2P/direct-device sync is not the current mainline|Pending expansion rejects P2P/signaling as an Actro backend track"
  "internal/browser-sync-roadmap.md|P2P/direct-device sync is not the current mainline|Browser sync roadmap rejects P2P/signaling as an Actro backend track"
  "internal/pending-expansion.md|synchronization substrate only|Pending expansion records that shared files do not replace Rust sync semantics"
  "internal/browser-sync-roadmap.md|synchronization substrate only|Browser sync roadmap records that shared files do not replace Rust sync semantics"
  "internal/browser-sync-roadmap.md|browser sync backend plus clients|Browser sync roadmap rejects the old backend-plus-clients framing"
  "internal/pending-expansion.md|selected provider SDK or cloud-storage adapter surface|Pending expansion records provider auth/TLS/quota as adapter concerns"
  "internal/browser-sync-roadmap.md|selected provider SDK or cloud-storage adapter surface|Browser sync roadmap records provider auth/TLS/quota as adapter concerns"
  "internal/pending-expansion.md|browser_storage_session_tab_ordering_plan_json|Pending expansion records Rust-owned session tab ordering"
  "internal/browser-sync-roadmap.md|browser_storage_session_tab_ordering_plan_json|Browser sync roadmap records Rust-owned session tab ordering"
  "internal/pending-expansion.md|orderingAction|Pending expansion records session tab ordering diagnostics"
  "internal/pending-expansion.md|orderingSummary|Pending expansion records session tab ordering summaries"
  "internal/browser-sync-roadmap.md|orderingAction|Browser sync roadmap records session tab ordering diagnostics"
  "internal/browser-sync-roadmap.md|orderingSummary|Browser sync roadmap records session tab ordering summaries"
  "internal/pending-expansion.md|browser_storage_bookmark_restore_placement_plan_json|Pending expansion records Rust-owned bookmark restore placement"
  "internal/browser-sync-roadmap.md|browser_storage_bookmark_restore_placement_plan_json|Browser sync roadmap records Rust-owned bookmark restore placement"
  "internal/pending-expansion.md|stateConflictAction|Pending expansion records bookmark delete/restore conflict diagnostics"
  "internal/pending-expansion.md|stateConflicts|Pending expansion records bookmark delete/restore conflict details"
  "internal/pending-expansion.md|stateConflictSummary|Pending expansion records bookmark delete/restore conflict summaries"
  "internal/pending-expansion.md|placementReason|Pending expansion records bookmark restore placement reasons"
  "internal/pending-expansion.md|stateConflictReason|Pending expansion records bookmark delete/restore conflict reasons"
  "internal/browser-sync-roadmap.md|stateConflictAction|Browser sync roadmap records bookmark delete/restore conflict diagnostics"
  "internal/browser-sync-roadmap.md|stateConflicts|Browser sync roadmap records bookmark delete/restore conflict details"
  "internal/browser-sync-roadmap.md|stateConflictSummary|Browser sync roadmap records bookmark delete/restore conflict summaries"
  "internal/browser-sync-roadmap.md|placementReason|Browser sync roadmap records bookmark restore placement reasons"
  "internal/browser-sync-roadmap.md|stateConflictReason|Browser sync roadmap records bookmark delete/restore conflict reasons"
  "internal/pending-expansion.md|browser_storage_bookmark_ownership_plan_json|Pending expansion records Rust-owned bookmark ownership/source resolution"
  "internal/pending-expansion.md|retentionReason|Pending expansion records bookmark ownership retention reason diagnostics"
  "internal/pending-expansion.md|suppressedBookmarkIds|Pending expansion records bookmark ownership suppressed source diagnostics"
  "internal/pending-expansion.md|ownershipSummary|Pending expansion records bookmark ownership summaries"
  "internal/browser-sync-roadmap.md|browser_storage_bookmark_ownership_plan_json|Browser sync roadmap records Rust-owned bookmark ownership/source resolution"
  "internal/browser-sync-roadmap.md|retentionReason|Browser sync roadmap records bookmark ownership retention reason diagnostics"
  "internal/browser-sync-roadmap.md|suppressedBookmarkIds|Browser sync roadmap records bookmark ownership suppressed source diagnostics"
  "internal/browser-sync-roadmap.md|ownershipSummary|Browser sync roadmap records bookmark ownership summaries"
  "internal/pending-expansion.md|browser_storage_session_active_tab_consistency_plan_json|Pending expansion records Rust-owned session active-tab consistency"
  "internal/browser-sync-roadmap.md|browser_storage_session_active_tab_consistency_plan_json|Browser sync roadmap records Rust-owned session active-tab consistency"
  "internal/pending-expansion.md|within-range, clamped, empty-session|Pending expansion records session active-tab consistency diagnostics"
  "internal/browser-sync-roadmap.md|consistencyReason|Browser sync roadmap records session active-tab consistency diagnostics"
  "internal/pending-expansion.md|consistencySummary|Pending expansion records session active-tab consistency summaries"
  "internal/browser-sync-roadmap.md|consistencySummary|Browser sync roadmap records session active-tab consistency summaries"
  "internal/pending-expansion.md|aggregationAction|Pending expansion records history visit aggregation diagnostics"
  "internal/pending-expansion.md|aggregationSummary|Pending expansion records history visit aggregation summaries"
  "internal/browser-sync-roadmap.md|aggregationAction|Browser sync roadmap records history visit aggregation diagnostics"
  "internal/browser-sync-roadmap.md|aggregationSummary|Browser sync roadmap records history visit aggregation summaries"
  "internal/pending-expansion.md|integrityAction|Pending expansion records bookmark folder integrity diagnostics"
  "internal/browser-sync-roadmap.md|integrityAction|Browser sync roadmap records bookmark folder integrity diagnostics"
  "internal/pending-expansion.md|integritySummary|Pending expansion records bookmark folder integrity summaries"
  "internal/browser-sync-roadmap.md|integritySummary|Browser sync roadmap records bookmark folder integrity summaries"
  "internal/pending-expansion.md|browser_storage_retention_pruning_plan_json|Pending expansion records Rust-owned retention pruning"
  "internal/browser-sync-roadmap.md|browser_storage_retention_pruning_plan_json|Browser sync roadmap records Rust-owned retention pruning"
  "internal/pending-expansion.md|keep/prune/tombstone decisions|Pending expansion records retention pruning diagnostics"
  "internal/browser-sync-roadmap.md|keep/prune/tombstone decisions|Browser sync roadmap records retention pruning diagnostics"
  "internal/pending-expansion.md|retentionSummary|Pending expansion records retention pruning summaries"
  "internal/browser-sync-roadmap.md|retentionSummary|Browser sync roadmap records retention pruning summaries"
  "internal/pending-expansion.md|maxHistoryPagesPerOriginDevice|Pending expansion records per-origin-device history pruning"
  "internal/browser-sync-roadmap.md|maxHistoryPagesPerOriginDevice|Browser sync roadmap records per-origin-device history pruning"
  "internal/pending-expansion.md|maxSessionTabsPerOriginDevice|Pending expansion records per-origin-device session-tab pruning"
  "internal/browser-sync-roadmap.md|maxSessionTabsPerOriginDevice|Browser sync roadmap records per-origin-device session-tab pruning"
  "internal/pending-expansion.md|membership remain product work|Pending expansion keeps provider/device integration after retiring the account server"
  "internal/pending-expansion.md|Cloud log/snapshot retention|Pending expansion keeps cloud log/snapshot retention instead of server cursor retention"
  "internal/pending-expansion.md|Merge semantics do not disappear|Pending expansion keeps Rust-owned sync semantics over shared files"
  "internal/pending-expansion.md|Cloud-drive provider, data format, and read/write planning|Pending expansion records the current cloud-drive provider/data-format mainline"
  "internal/pending-expansion.md|first cloud-drive push write plan|Pending expansion records the cloud push write planner"
  "internal/pending-expansion.md|writeReason|Pending expansion records cloud planned write reasons"
  "internal/pending-expansion.md|writeSummary|Pending expansion records cloud push write summaries"
  "internal/pending-expansion.md|first cloud-drive push result handoff|Pending expansion records the cloud push result handoff"
  "internal/pending-expansion.md|failed-write issues preserving|Pending expansion records push result write reasons"
  "internal/pending-expansion.md|ackReason|Pending expansion records cloud push ack reasons"
  "internal/pending-expansion.md|first cloud-drive pull/read plan|Pending expansion records the cloud pull read planner"
  "internal/pending-expansion.md|readReason|Pending expansion records cloud planned read reasons"
  "internal/pending-expansion.md|readSummary|Pending expansion records cloud pull read summaries"
  "internal/pending-expansion.md|readListedCloudLogObject|Pending expansion records cloud read-result reasons"
  "internal/pending-expansion.md|first cloud-drive pull apply handoff|Pending expansion records the cloud pull apply handoff"
  "internal/pending-expansion.md|skippedSnapshots|Pending expansion records cloud pull apply skipped-snapshot diagnostics"
  "internal/pending-expansion.md|skippedLogs|Pending expansion records cloud pull apply skipped-log diagnostics"
  "internal/pending-expansion.md|applyAction|Pending expansion records cloud pull apply action diagnostics"
  "internal/pending-expansion.md|applySummary|Pending expansion records cloud pull apply summaries"
  "internal/pending-expansion.md|first cloud-drive pull pagination continuation plan|Pending expansion records the cloud pull continuation planner"
  "internal/pending-expansion.md|continuationSummary|Pending expansion records cloud pull continuation summaries"
  "internal/pending-expansion.md|browser_storage_sync_cloud_execute_planned_reads_with_continuations|Pending expansion records Rust-backed pull pagination execution"
  "internal/pending-expansion.md|external provider SDK pagination loops|Pending expansion narrows remaining pagination work to external provider SDK adapters"
  "internal/pending-expansion.md|first snapshot compaction planner|Pending expansion records the cloud compaction planner"
  "internal/pending-expansion.md|first cloud-drive compaction result handoff|Pending expansion records the cloud compaction result handoff"
  "internal/pending-expansion.md|compaction write/delete failures preserve|Pending expansion records compaction result issue reasons"
  "internal/pending-expansion.md|compaction \`roundSummary\`|Pending expansion records cloud compaction round summaries"
  "internal/pending-expansion.md|first cloud high-watermark/retention status planner|Pending expansion records the cloud high-watermark retention planner"
  "internal/pending-expansion.md|deleteReason|Pending expansion records cloud planned delete reasons"
  "internal/pending-expansion.md|first cloud cleanup result handoff|Pending expansion records the cloud cleanup result handoff"
  "internal/pending-expansion.md|cleanup delete failures preserve|Pending expansion records cleanup result delete reasons"
  "internal/pending-expansion.md|cleanup \`roundSummary\`|Pending expansion records cloud cleanup round summaries"
  "internal/pending-expansion.md|first in-memory cloud store fixture|Pending expansion records the in-memory cloud store fixture"
  "internal/pending-expansion.md|first file-backed cloud store fixture|Pending expansion records the local-folder cloud store fixture"
  "internal/pending-expansion.md|planned operation executor|Pending expansion records the cloud planned operation executor"
  "internal/pending-expansion.md|first cloud push round runner|Pending expansion records the cloud push round runner"
  "internal/pending-expansion.md|push \`roundSummary\`|Pending expansion records cloud push round summaries"
  "internal/pending-expansion.md|first cloud pull round runner|Pending expansion records the cloud pull round runner"
  "internal/pending-expansion.md|roundSummary|Pending expansion records cloud pull round summaries"
  "internal/pending-expansion.md|first cloud cycle round runner|Pending expansion records the cloud cycle round runner"
  "internal/pending-expansion.md|cycle \`roundSummary\`|Pending expansion records cloud cycle round summaries"
  "internal/pending-expansion.md|first cloud cycle client plan|Pending expansion records the cloud cycle client plan"
  "internal/pending-expansion.md|stepReason|Pending expansion records cloud cycle client step reasons"
  "internal/pending-expansion.md|first cloud cycle status policy|Pending expansion records the cloud cycle status policy"
  "internal/pending-expansion.md|first cloud membership status policy|Pending expansion records the cloud membership status policy"
  "internal/pending-expansion.md|first cloud cycle schedule state|Pending expansion records the cloud cycle schedule state"
  "internal/pending-expansion.md|first cloud membership schedule state|Pending expansion records the cloud membership schedule state"
  "internal/pending-expansion.md|first cloud membership attempt plan|Pending expansion records the cloud membership attempt plan"
  "internal/pending-expansion.md|first cloud membership loop plan|Pending expansion records the cloud membership loop plan"
  "internal/pending-expansion.md|membership \`loopSummary\`|Pending expansion records cloud membership loop summaries"
  "internal/pending-expansion.md|cloud provider error classification policy|Pending expansion records the cloud provider error classifier"
  "internal/pending-expansion.md|providerErrorClassification|Pending expansion records provider error diagnostics source mapping"
  "internal/pending-expansion.md|first cloud provider diagnostics summary|Pending expansion records the cloud provider diagnostics summary"
  "internal/pending-expansion.md|first cloud observability event contract|Pending expansion records the cloud observability event contract"
  "internal/pending-expansion.md|first cloud cycle attempt plan|Pending expansion records the cloud cycle attempt plan"
  "internal/pending-expansion.md|first cloud cycle loop plan|Pending expansion records the cloud cycle loop plan"
  "internal/pending-expansion.md|cycle \`loopSummary\`|Pending expansion records cloud cycle loop summaries"
  "internal/pending-expansion.md|local-folder cloud store as a coarse native-callable cycle entry|Pending expansion records the local-folder cycle JSON entry"
  "internal/pending-expansion.md|local-folder cloud store as a coarse native-callable membership entry|Pending expansion records the local-folder membership JSON entry"
  "internal/pending-expansion.md|local-folder cloud-file compatibility preflight follow-up|Pending expansion records the local-folder compatibility preflight follow-up"
  "internal/pending-expansion.md|browser_storage_sync_cloud_local_folder_compatibility_preflight_json|Pending expansion records the local-folder compatibility preflight helper"
  "internal/pending-expansion.md|local-folder membership compatibility gate follow-up|Pending expansion records the local-folder membership compatibility gate follow-up"
  "internal/pending-expansion.md|moves membership write blocking into the existing Rust membership round|Pending expansion records Rust-gated local-folder membership writes"
  "internal/pending-expansion.md|local-folder membership compatibility gate follow-up was verified|Pending expansion records local-folder membership gate verification"
  "internal/pending-expansion.md|first browser storage address canonicalization policy|Pending expansion records the address canonicalization policy"
  "internal/pending-expansion.md|credentialStripped|Pending expansion records URL credential stripping diagnostics"
  "internal/pending-expansion.md|canonicalizationSummary|Pending expansion records address canonicalization summaries"
  "internal/pending-expansion.md|first cloud-store capability preflight|Pending expansion records the cloud store capability preflight"
  "internal/pending-expansion.md|first cloud-file compatibility status gate|Pending expansion records the cloud compatibility gate"
  "internal/pending-expansion.md|batched cloud-file compatibility preflight|Pending expansion records the batched cloud compatibility preflight"
  "internal/pending-expansion.md|first cloud schema rollout plan|Pending expansion records the cloud schema rollout plan"
  "internal/pending-expansion.md|first cloud schema migration playbook|Pending expansion records the cloud schema migration playbook"
  "internal/pending-expansion.md|first cloud recovery snapshot plan|Pending expansion records the cloud recovery snapshot plan"
  "internal/pending-expansion.md|cloud recovery snapshot result handoff|Pending expansion records the cloud recovery snapshot result handoff"
  "internal/pending-expansion.md|coarse recovery snapshot round runner|Pending expansion records the cloud recovery snapshot round runner"
  "internal/pending-expansion.md|browser_storage_sync_cloud_local_folder_recovery_snapshot_round_json|Pending expansion records the local-folder recovery snapshot round entry"
  "internal/pending-expansion.md|recovery snapshot \`roundSummary\`|Pending expansion records recovery snapshot round summaries"
  "internal/pending-expansion.md|writeRecoveryCopy|Pending expansion records cloud recovery snapshot copy operations"
  "internal/pending-expansion.md|first cloud device membership write/result path|Pending expansion records the cloud device membership planner"
  "internal/pending-expansion.md|cloud device membership result handoff|Pending expansion records the cloud device membership result handoff"
  "internal/pending-expansion.md|browser_storage_sync_cloud_run_membership_round|Pending expansion records the cloud membership round runner"
  "internal/pending-expansion.md|membership \`roundSummary\`|Pending expansion records cloud membership round summaries"
  "internal/pending-expansion.md|first bookmark folder tree integrity policy|Pending expansion records the bookmark folder tree integrity policy"
  "internal/pending-expansion.md|Provider connection and cloud device membership|Pending expansion records provider connection and cloud device membership mainline"
  "internal/pending-expansion.md|Windows parity and packaging|Pending expansion records Windows parity and packaging mainline"
  "internal/pending-expansion.md|No client/server rollout switch|Pending expansion rejects client/server sync cutover framing"
  "internal/browser-sync-roadmap.md|Product direction correction|Browser sync roadmap records the cloud-drive product correction"
  "internal/browser-sync-roadmap.md|provider-owned file sync|Browser sync roadmap records provider-owned file sync as the replacement mainline"
  "internal/browser-sync-roadmap.md|replacement memory for the quoted 2026-07-03 seven-point|Browser sync roadmap retires the old seven-point hosted-sync backlog"
  "internal/browser-sync-roadmap.md|Retired from the mainline|Browser sync roadmap explicitly retires the old hosted-sync backlog"
  "internal/browser-sync-roadmap.md|BrowserStorageSyncCloudStore|Browser sync roadmap records the provider-neutral Rust store trait"
  "internal/browser-sync-roadmap.md|not a first-party browser sync account|Browser sync roadmap rejects service-side device registration and revocation"
  "internal/browser-sync-roadmap.md|device-management API target|Browser sync roadmap rejects first-party account/device service"
  "internal/browser-sync-roadmap.md|vanish into generic file sync|Browser sync roadmap keeps provider/device integration after retiring the account server"
  "internal/browser-sync-roadmap.md|Cloud-drive provider, data format, and read/write planning|Browser sync roadmap records the current cloud-drive provider/data-format mainline"
  "internal/browser-sync-roadmap.md|cloud-drive push write plan|Browser sync roadmap records the cloud push write planner"
  "internal/browser-sync-roadmap.md|writeReason|Browser sync roadmap records cloud planned write reasons"
  "internal/browser-sync-roadmap.md|writeSummary|Browser sync roadmap records cloud push write summaries"
  "internal/browser-sync-roadmap.md|cloud-drive push result handoff|Browser sync roadmap records the cloud push result handoff"
  "internal/browser-sync-roadmap.md|failed-write issues preserving|Browser sync roadmap records push result write reasons"
  "internal/browser-sync-roadmap.md|ackReason|Browser sync roadmap records cloud push ack reasons"
  "internal/browser-sync-roadmap.md|cloud-drive pull/read plan|Browser sync roadmap records the cloud pull read planner"
  "internal/browser-sync-roadmap.md|readReason|Browser sync roadmap records cloud planned read reasons"
  "internal/browser-sync-roadmap.md|readSummary|Browser sync roadmap records cloud pull read summaries"
  "internal/browser-sync-roadmap.md|readListedCloudLogObject|Browser sync roadmap records cloud read-result reasons"
  "internal/browser-sync-roadmap.md|cloud-drive pull apply handoff|Browser sync roadmap records the cloud pull apply handoff"
  "internal/browser-sync-roadmap.md|skippedSnapshots|Browser sync roadmap records cloud pull apply skipped-snapshot diagnostics"
  "internal/browser-sync-roadmap.md|skippedLogs|Browser sync roadmap records cloud pull apply skipped-log diagnostics"
  "internal/browser-sync-roadmap.md|applyAction|Browser sync roadmap records cloud pull apply action diagnostics"
  "internal/browser-sync-roadmap.md|applySummary|Browser sync roadmap records cloud pull apply summaries"
  "internal/browser-sync-roadmap.md|cloud-drive pull pagination continuation plan|Browser sync roadmap records the cloud pull continuation planner"
  "internal/browser-sync-roadmap.md|continuationSummary|Browser sync roadmap records cloud pull continuation summaries"
  "internal/browser-sync-roadmap.md|browser_storage_sync_cloud_execute_planned_reads_with_continuations|Browser sync roadmap records Rust-backed pull pagination execution"
  "internal/browser-sync-roadmap.md|external provider SDK pagination loops|Browser sync roadmap narrows remaining pagination work to external provider SDK adapters"
  "internal/browser-sync-roadmap.md|snapshot compaction planner|Browser sync roadmap records the cloud compaction planner"
  "internal/browser-sync-roadmap.md|cloud-drive compaction result handoff|Browser sync roadmap records the cloud compaction result handoff"
  "internal/browser-sync-roadmap.md|result issues preserving planned|Browser sync roadmap records compaction result issue reasons"
  "internal/browser-sync-roadmap.md|compaction \`roundSummary\`|Browser sync roadmap records cloud compaction round summaries"
  "internal/browser-sync-roadmap.md|cloud high-watermark/retention status planner|Browser sync roadmap records the cloud high-watermark retention planner"
  "internal/browser-sync-roadmap.md|deleteReason|Browser sync roadmap records cloud planned delete reasons"
  "internal/browser-sync-roadmap.md|cloud cleanup result handoff|Browser sync roadmap records the cloud cleanup result handoff"
  "internal/browser-sync-roadmap.md|delete issues preserve|Browser sync roadmap records cleanup result delete reasons"
  "internal/browser-sync-roadmap.md|cleanup \`roundSummary\`|Browser sync roadmap records cloud cleanup round summaries"
  "internal/browser-sync-roadmap.md|in-memory cloud store fixture|Browser sync roadmap records the in-memory cloud store fixture"
  "internal/browser-sync-roadmap.md|file-backed cloud store fixture|Browser sync roadmap records the local-folder cloud store fixture"
  "internal/browser-sync-roadmap.md|planned operation executor|Browser sync roadmap records the cloud planned operation executor"
  "internal/browser-sync-roadmap.md|cloud push round runner|Browser sync roadmap records the cloud push round runner"
  "internal/browser-sync-roadmap.md|push \`roundSummary\`|Browser sync roadmap records cloud push round summaries"
  "internal/browser-sync-roadmap.md|cloud pull round runner|Browser sync roadmap records the cloud pull round runner"
  "internal/browser-sync-roadmap.md|roundSummary|Browser sync roadmap records cloud pull round summaries"
  "internal/browser-sync-roadmap.md|cloud cycle round runner|Browser sync roadmap records the cloud cycle round runner"
  "internal/browser-sync-roadmap.md|cycle \`roundSummary\`|Browser sync roadmap records cloud cycle round summaries"
  "internal/browser-sync-roadmap.md|cloud cycle client plan|Browser sync roadmap records the cloud cycle client plan"
  "internal/browser-sync-roadmap.md|stepReason|Browser sync roadmap records cloud cycle client step reasons"
  "internal/browser-sync-roadmap.md|cloud cycle status policy|Browser sync roadmap records the cloud cycle status policy"
  "internal/browser-sync-roadmap.md|cloud membership status policy|Browser sync roadmap records the cloud membership status policy"
  "internal/browser-sync-roadmap.md|cloud cycle schedule state|Browser sync roadmap records the cloud cycle schedule state"
  "internal/browser-sync-roadmap.md|cloud membership schedule state|Browser sync roadmap records the cloud membership schedule state"
  "internal/browser-sync-roadmap.md|cloud membership attempt plan|Browser sync roadmap records the cloud membership attempt plan"
  "internal/browser-sync-roadmap.md|cloud membership loop plan|Browser sync roadmap records the cloud membership loop plan"
  "internal/browser-sync-roadmap.md|membership \`loopSummary\`|Browser sync roadmap records cloud membership loop summaries"
  "internal/browser-sync-roadmap.md|cloud provider error classification policy|Browser sync roadmap records the cloud provider error classifier"
  "internal/browser-sync-roadmap.md|providerErrorClassification|Browser sync roadmap records provider error diagnostics source mapping"
  "internal/browser-sync-roadmap.md|cloud provider diagnostics summary|Browser sync roadmap records the cloud provider diagnostics summary"
  "internal/browser-sync-roadmap.md|cloud observability event contract|Browser sync roadmap records the cloud observability event contract"
  "internal/browser-sync-roadmap.md|cloud cycle attempt plan|Browser sync roadmap records the cloud cycle attempt plan"
  "internal/browser-sync-roadmap.md|cloud cycle loop plan|Browser sync roadmap records the cloud cycle loop plan"
  "internal/browser-sync-roadmap.md|cycle \`loopSummary\`|Browser sync roadmap records cloud cycle loop summaries"
  "internal/browser-sync-roadmap.md|local-folder cloud store as a coarse native-callable cycle entry|Browser sync roadmap records the local-folder cycle JSON entry"
  "internal/browser-sync-roadmap.md|local-folder cloud store as a coarse native-callable membership entry|Browser sync roadmap records the local-folder membership JSON entry"
  "internal/browser-sync-roadmap.md|local-folder cloud-file compatibility preflight follow-up|Browser sync roadmap records the local-folder compatibility preflight follow-up"
  "internal/browser-sync-roadmap.md|browser_storage_sync_cloud_local_folder_compatibility_preflight_json|Browser sync roadmap records the local-folder compatibility preflight helper"
  "internal/browser-sync-roadmap.md|local-folder membership compatibility gate follow-up|Browser sync roadmap records the local-folder membership compatibility gate follow-up"
  "internal/browser-sync-roadmap.md|gates membership writes with|Browser sync roadmap records Rust-gated membership writes"
  "internal/browser-sync-roadmap.md|Verification for this membership gate|Browser sync roadmap records local-folder membership gate verification"
  "internal/browser-sync-roadmap.md|browser storage address canonicalization policy|Browser sync roadmap records the address canonicalization policy"
  "internal/browser-sync-roadmap.md|credentialStripped|Browser sync roadmap records URL credential stripping diagnostics"
  "internal/browser-sync-roadmap.md|canonicalizationSummary|Browser sync roadmap records address canonicalization summaries"
  "internal/browser-sync-roadmap.md|cloud-store capability preflight|Browser sync roadmap records the cloud store capability preflight"
  "internal/browser-sync-roadmap.md|cloud-file compatibility status gate|Browser sync roadmap records the cloud compatibility gate"
  "internal/browser-sync-roadmap.md|batched cloud-file compatibility preflight|Browser sync roadmap records the batched cloud compatibility preflight"
  "internal/browser-sync-roadmap.md|cloud schema rollout plan|Browser sync roadmap records the cloud schema rollout plan"
  "internal/browser-sync-roadmap.md|cloud schema migration playbook|Browser sync roadmap records the cloud schema migration playbook"
  "internal/browser-sync-roadmap.md|cloud recovery snapshot plan|Browser sync roadmap records the cloud recovery snapshot plan"
  "internal/browser-sync-roadmap.md|cloud recovery snapshot result handoff|Browser sync roadmap records the cloud recovery snapshot result handoff"
  "internal/browser-sync-roadmap.md|coarse recovery snapshot round runner|Browser sync roadmap records the cloud recovery snapshot round runner"
  "internal/browser-sync-roadmap.md|browser_storage_sync_cloud_local_folder_recovery_snapshot_round_json|Browser sync roadmap records the local-folder recovery snapshot round entry"
  "internal/browser-sync-roadmap.md|recovery snapshot \`roundSummary\`|Browser sync roadmap records recovery snapshot round summaries"
  "internal/browser-sync-roadmap.md|writeRecoveryCopy|Browser sync roadmap records cloud recovery snapshot copy operations"
  "internal/browser-sync-roadmap.md|cloud device membership write/result path|Browser sync roadmap records the cloud device membership planner"
  "internal/browser-sync-roadmap.md|BrowserStorageSyncCloudMembershipResultPlan|Browser sync roadmap records the cloud device membership result handoff"
  "internal/browser-sync-roadmap.md|browser_storage_sync_cloud_run_membership_round|Browser sync roadmap records the cloud membership round runner"
  "internal/browser-sync-roadmap.md|membership \`roundSummary\`|Browser sync roadmap records cloud membership round summaries"
  "internal/browser-sync-roadmap.md|bookmark folder tree integrity policy|Browser sync roadmap records the bookmark folder tree integrity policy"
  "internal/browser-sync-roadmap.md|Provider connection and cloud device membership|Browser sync roadmap records provider connection and cloud device membership mainline"
  "internal/browser-sync-roadmap.md|files does not remove the need for Rust-owned deterministic rules|Browser sync roadmap keeps Rust-owned sync semantics over shared files"
  "internal/browser-sync-roadmap.md|There is no client/server|Browser sync roadmap rejects client/server sync cutover framing"
  "internal/pending-expansion.md|Replacement for the retired seven-item hosted-sync mainline|Pending expansion records the updated seven-item cloud-drive mainline"
  "internal/pending-expansion.md|no built-in sync server|Pending expansion rejects an embedded browser sync server"
  "internal/pending-expansion.md|substrate, not the product semantics|Pending expansion keeps browser sync semantics above file transport"
  "internal/pending-expansion.md|Provider/file production readiness|Pending expansion records provider/file production readiness"
  "internal/pending-expansion.md|old hosted-backend item|Pending expansion maps the old backend item to provider-file readiness"
  "internal/pending-expansion.md|Account/device integration is retired as an Actro service track|Pending expansion maps account/device work away from an Actro service"
  "internal/pending-expansion.md|selected cloud provider account/session|Pending expansion clarifies account means cloud provider account/session"
  "internal/pending-expansion.md|File sync shares bytes|Pending expansion keeps sync semantics beyond file sync"
  "internal/pending-expansion.md|file repair/migration playbooks|Pending expansion maps old backup/migration work to provider file readiness"
  "internal/pending-expansion.md|Provider connection and cloud membership|Pending expansion records provider connection and cloud membership"
  "internal/pending-expansion.md|no first-party browser-sync account|Pending expansion rejects first-party browser-sync accounts"
  "internal/pending-expansion.md|device revoke API|Pending expansion rejects service-side device revocation"
  "internal/pending-expansion.md|sharing file structures does not remove|Pending expansion keeps provider membership work without an account server"
  "internal/pending-expansion.md|Schema governance remains only as client/cloud-file schema rollout|Pending expansion records client/cloud-file schema governance"
  "internal/pending-expansion.md|Actro-operated browser-sync infrastructure|Pending expansion retires Actro-operated browser sync infrastructure"
  "internal/pending-expansion.md|tracks are not renamed tasks|Pending expansion rejects hidden service-backed follow-up tracks"
  "internal/pending-expansion.md|not an Actro service API|Pending expansion maps provider membership away from a service API"
  "internal/pending-expansion.md|Merge semantics do not disappear|Pending expansion keeps deterministic sync rules above shared files"
  "internal/pending-expansion.md|not solved by file sync alone|Pending expansion keeps product semantics above shared cloud files"
  "internal/pending-expansion.md|Current exact-segment replacement|Pending expansion records the exact seven-item replacement memory"
  "internal/pending-expansion.md|service device|Pending expansion rejects a service device registry track"
  "internal/pending-expansion.md|Shared provider files|Pending expansion records shared provider files as the sync substrate"
  "internal/pending-expansion.md|2026-07-04 canonical correction|Pending expansion records the latest seven-item cloud-file replacement"
  "internal/pending-expansion.md|no built-in service, no Actro-operated production backend|Pending expansion rejects built-in browser sync services"
  "internal/pending-expansion.md|Shared files are the synchronization medium|Pending expansion keeps rules above shared file transport"
  "internal/pending-expansion.md|Cloud providers such as iCloud Drive, Google Drive, OneDrive|Pending expansion records cloud providers as user-owned storage substrates"
  "internal/pending-expansion.md|connecting the same provider account|Pending expansion records same-provider-account sync setup"
  "internal/browser-sync-roadmap.md|Replacement for the retired seven-item hosted-sync mainline|Browser sync roadmap records the updated seven-item cloud-drive mainline"
  "internal/browser-sync-roadmap.md|no built-in sync server|Browser sync roadmap rejects an embedded browser sync server"
  "internal/browser-sync-roadmap.md|substrate, not the product semantics|Browser sync roadmap keeps browser sync semantics above file transport"
  "internal/browser-sync-roadmap.md|Cloud providers such as iCloud Drive, Google Drive, OneDrive|Browser sync roadmap records cloud providers as user-owned storage substrates"
  "internal/browser-sync-roadmap.md|connecting the same provider account|Browser sync roadmap records same-provider-account sync setup"
  "internal/browser-sync-roadmap.md|2026-07-04 canonical replacement|Browser sync roadmap records the latest seven-item cloud-file replacement"
  "internal/browser-sync-roadmap.md|There is no first-party browser-sync account source|Browser sync roadmap rejects service account/device sources"
  "internal/browser-sync-roadmap.md|File sync shares bytes|Browser sync roadmap keeps deterministic sync rules above file transport"
  "internal/browser-sync-roadmap.md|Provider/file production readiness|Browser sync roadmap records provider/file production readiness"
  "internal/browser-sync-roadmap.md|old hosted-backend item|Browser sync roadmap maps the old backend item to provider-file readiness"
  "internal/browser-sync-roadmap.md|Account/device integration is retired as an Actro service track|Browser sync roadmap maps account/device work away from an Actro service"
  "internal/browser-sync-roadmap.md|selected cloud provider account/session|Browser sync roadmap clarifies account means cloud provider account/session"
  "internal/browser-sync-roadmap.md|File sync shares bytes|Browser sync roadmap keeps sync semantics beyond file sync"
  "internal/browser-sync-roadmap.md|file repair/migration playbooks|Browser sync roadmap maps old backup/migration work to provider file readiness"
  "internal/browser-sync-roadmap.md|tracks are not renamed tasks|Browser sync roadmap rejects hidden service-backed follow-up tracks"
  "internal/browser-sync-roadmap.md|Provider connection and cloud membership|Browser sync roadmap records provider connection and cloud membership"
  "internal/browser-sync-roadmap.md|no first-party browser-sync account|Browser sync roadmap rejects first-party browser-sync accounts"
  "internal/browser-sync-roadmap.md|device revoke API|Browser sync roadmap rejects service-side device revocation"
  "internal/browser-sync-roadmap.md|sharing file structures does not remove|Browser sync roadmap keeps provider membership work without an account server"
  "internal/browser-sync-roadmap.md|Schema evolution remains only as client/cloud-file governance|Browser sync roadmap records client/cloud-file schema governance"
  "internal/browser-sync-roadmap.md|Actro-operated browser-sync infrastructure|Browser sync roadmap retires Actro-operated browser sync infrastructure"
  "internal/browser-sync-roadmap.md|There is no Actro product account|Browser sync roadmap maps old account-device work to provider membership"
  "internal/browser-sync-roadmap.md|Merge semantics do not disappear|Browser sync roadmap keeps deterministic sync rules above shared files"
  "internal/browser-sync-roadmap.md|not solved by file sync alone|Browser sync roadmap keeps product semantics above shared cloud files"
  "internal/browser-sync-roadmap.md|Current exact-segment replacement|Browser sync roadmap records the exact seven-item replacement memory"
  "internal/browser-sync-roadmap.md|service device registry|Browser sync roadmap rejects a service device registry track"
  "internal/browser-sync-roadmap.md|Shared provider files|Browser sync roadmap records shared provider files as the sync substrate"
  "internal/pending-expansion.md|Latest whole-paragraph memory lock|Pending expansion records the latest quoted backlog replacement"
  "internal/browser-sync-roadmap.md|Latest whole-paragraph memory lock|Browser sync roadmap records the latest quoted backlog replacement"
  "internal/pending-expansion.md|provider session plus local secure storage|Pending expansion maps account/device work to provider sessions and local storage"
  "internal/browser-sync-roadmap.md|provider session plus local secure storage|Browser sync roadmap maps account/device work to provider sessions and local storage"
  "internal/pending-expansion.md|client/cloud-file compatibility and recovery|Pending expansion maps schema evolution to client/cloud-file recovery"
  "internal/browser-sync-roadmap.md|client/cloud-file compatibility and recovery|Browser sync roadmap maps schema evolution to client/cloud-file recovery"
)

for requirement in "${browser_sync_product_direction_requirements[@]}"; do
  file="${requirement%%|*}"
  remainder="${requirement#*|}"
  required_pattern="${remainder%%|*}"
  description="${remainder##*|}"

  if [[ ! -f "$file" ]]; then
    print_failure "$description file is missing"
  elif ! rg -q "$required_pattern" "$file"; then
    print_failure "$description"
  else
    print_ok "$description"
  fi
done

browser_sync_old_backend_direction="$(
  rg -n 'Production browser sync backend|production browser sync backend|production backend deployment|deployable hosting/TLS|server-backed device registration|server-side device registration|server-side device revocation|account/device service integration|first-party browser-sync account/device registry|client/server cutover|production auth integration|production transaction/error policy|生产 browser sync 后端|生产 browser sync 后台|服务端设备注册|服务端设备.*撤销|客户端/服务端切换|真实 auth|内置服务端' \
    internal/pending-expansion.md \
    internal/browser-sync-roadmap.md \
    docs/architecture/overview.md \
    docs/architecture/rust-core.md \
    docs/architecture/browser-sync-cloud-drive.md \
    2>/dev/null || true
)"

if [[ -n "$browser_sync_old_backend_direction" ]]; then
  print_failure "Browser sync roadmap reintroduces an Actro-operated sync path"
  printf '%s\n' "$browser_sync_old_backend_direction"
else
  print_ok "Browser sync roadmap keeps the product direction on user-owned cloud storage"
fi

browser_sync_embedded_server_refs="$(
  rg -n 'actro-sync-server|BrowserSyncService|BrowserSyncHttp(Request|Response)|register-device|tiny_http|Server::http' \
    apps \
    packages/actro-runtime \
    platform/apple \
    packages/actro-native-kotlin \
    2>/dev/null || true
)"

if [[ -n "$browser_sync_embedded_server_refs" ]]; then
  print_failure "Browser client paths reference the browser sync reference server"
  printf '%s\n' "$browser_sync_embedded_server_refs"
else
  print_ok "Browser client paths do not embed or reference the browser sync reference server"
fi

protected_native_files=(
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/core/ReactNativeBundle.kt"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/core/TabModule.kt"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/core/UserInfoModule.kt"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/event/NativeModuleEvent.kt"
  "apps/ios/actro/Actro/App/TabEventEmitter.swift"
  "apps/ios/actro/Actro/App/TabModule.swift"
  "apps/ios/actro/Actro/App/UserInfoModule.swift"
  "apps/ios/actro/Actro/Views/MainContentView/BrowserViewModel.swift"
  "apps/ios/actro/Actro/Views/MainContentView/ReactNativeTabHostView.swift"
  "apps/macos/actro/Actro/Views/MainContentView/BrowserViewModel.swift"
)

missing_files=()
for file in "${protected_native_files[@]}"; do
  if [[ ! -f "$file" ]]; then
    missing_files+=("$file")
  fi
done

if [[ "${#missing_files[@]}" -ne 0 ]]; then
  print_failure "protected native boundary files are missing"
  printf '%s\n' "${missing_files[@]}"
else
  print_ok "protected native boundary files exist"
fi

direct_native_imports="$(
  rg -n 'import ActroCore|import io\.actrium\.actro\.(rust|generated)\.ActroCore' \
    "${protected_native_files[@]}" 2>/dev/null || true
)"

if [[ -n "$direct_native_imports" ]]; then
  print_failure "adapter-protected RN/UI boundary files import ActroCore directly"
  printf '%s\n' "$direct_native_imports"
else
  print_ok "adapter-protected RN/UI boundary files avoid direct ActroCore imports"
fi

android_runtime_dto_consumers=(
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/MainViewModel.kt"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/tablet/TabletViewModel.kt"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/model/ActiveModel.kt"
)

android_generated_runtime_dto_imports="$(
  rg -n 'import io\.actrium\.actro\.rust\.Runtime(OpenTabPlanInfo|Tab(Update|Unread|BookmarkStatus)Info)' \
    "${android_runtime_dto_consumers[@]}" 2>/dev/null || true
)"

if [[ -n "$android_generated_runtime_dto_imports" ]]; then
  print_failure "Android ViewModel/model files consume generated Rust runtime DTOs directly"
  printf '%s\n' "$android_generated_runtime_dto_imports"
else
  print_ok "Android ViewModel/model files consume local runtime DTO adapters"
fi

android_protected_runtime_dto_imports="$(
  rg -n 'import io\.actrium\.actro\.rust\.Runtime(SystemRootProps|RootProps|Tab|OpenTabPlan|TabUpdate|TabUnread|TabBookmarkStatus)Info' \
    "${protected_native_files[@]}" 2>/dev/null || true
)"

if [[ -n "$android_protected_runtime_dto_imports" ]]; then
  print_failure "Android protected RN/UI boundary files consume generated Rust runtime DTOs directly"
  printf '%s\n' "$android_protected_runtime_dto_imports"
else
  print_ok "Android protected RN/UI boundary files consume local runtime DTO adapters"
fi

android_app_model_rust_imports="$(
  rg -n 'import io\.actrium\.actro\.rust\.|Runtime(Tab|RootProps|SystemRootProps)Info|ActroCore' \
    apps/android/actro/android/app/src/main/java/io/actrium/actro/model/AppModel.kt \
    2>/dev/null || true
)"

if [[ -n "$android_app_model_rust_imports" ]]; then
  print_failure "Android AppModel depends on Rust-generated runtime DTOs instead of TabBundleUtils"
  printf '%s\n' "$android_app_model_rust_imports"
else
  print_ok "Android AppModel delegates runtime DTO conversion to TabBundleUtils"
fi

android_active_model_rust_imports="$(
  rg -n 'import io\.actrium\.actro\.rust\.|ActiveTab(Row)?Info|ActroCore|toCoreInfo\(|fromCoreInfo\(|fromCoreRowInfo|toCoreRowInfo' \
    apps/android/actro/android/app/src/main/java/io/actrium/actro/model/ActiveModel.kt \
    apps/android/actro/android/app/src/main/java/io/actrium/actro/data/entity/ActiveEntity.kt \
    apps/android/actro/android/app/src/main/java/io/actrium/actro/data/repository/ActiveRepository.kt \
    2>/dev/null || true
)"

if [[ -n "$android_active_model_rust_imports" ]]; then
  print_failure "Android active tab model/data files depend on Rust-generated DTOs instead of ActiveTabCoreAdapter"
  printf '%s\n' "$android_active_model_rust_imports"
else
  print_ok "Android active tab model/data files delegate Rust DTO conversion to ActiveTabCoreAdapter"
fi

android_active_tab_overextraction="$(
  rg -n 'active_tab_apply_runtime_update|activeTabApplyRuntimeUpdate' \
    crates/actro-core/src \
    crates/actro-core/generated \
    platform/apple/packages/ActroCore/Sources \
    packages/actro-native-kotlin/src/main/kotlin \
    apps/android/actro/android/app/src/main/java/io/actrium/actro \
    2>/dev/null || true
)"

if [[ -n "$android_active_tab_overextraction" ]]; then
  print_failure "Android active-tab update merge leaked back into Rust/generated bindings"
  printf '%s\n' "$android_active_tab_overextraction"
else
  print_ok "Android active-tab update merge stays local to the Android model"
fi

android_browser_storage_file="apps/android/actro/android/app/src/main/java/io/actrium/actro/data/database/BrowserStorageDatabase.kt"
android_browser_data_repository_file="apps/android/actro/android/app/src/main/java/io/actrium/actro/data/repository/BrowserDataRepository.kt"
android_browser_data_module_file="apps/android/actro/android/app/src/main/java/io/actrium/actro/core/BrowserDataModule.kt"
if [[ ! -f "$android_browser_storage_file" ]]; then
  print_failure "Android shared browser storage adapter is missing"
elif ! rg -q 'ActroCore\.browserStorageSchema\(\)' "$android_browser_storage_file"; then
  print_failure "Android browser storage adapter does not consume Rust storage schema"
elif ! rg -q 'ActroCore\.browserStorageRepositorySql\(\)' "$android_browser_storage_file"; then
  print_failure "Android browser storage adapter does not consume Rust repository SQL"
elif ! rg -q 'SQLiteDatabase' "$android_browser_storage_file"; then
  print_failure "Android browser storage adapter does not own Android SQLite execution"
elif ! rg -q 'tab_id' "$android_browser_storage_file" ||
  ! rg -q 'syncSql\.' "$android_browser_storage_file" ||
  ! rg -q 'browser_sync_cursors' "$android_browser_storage_file" ||
  ! rg -q 'migrateBrowserStorageSchemaIfNeeded' "$android_browser_storage_file"; then
  print_failure "Android browser storage adapter is missing browser session/sync migration readwrite"
else
  print_ok "Android browser storage adapter consumes Rust schema through SQLiteDatabase"
fi

if [[ ! -f "$android_browser_data_repository_file" ]]; then
  print_failure "Android browser data repository is missing"
elif ! rg -q 'BrowserStorageDatabase' "$android_browser_data_repository_file"; then
  print_failure "Android browser data repository does not consume the browser storage adapter"
elif ! rg -q 'recordHistoryVisit' "$android_browser_data_repository_file"; then
  print_failure "Android browser data repository is missing history visit browser storage calls"
elif ! rg -q 'sql\.deleteBookmarkById' "$android_browser_storage_file"; then
  print_failure "Android browser storage adapter does not use the Rust contract id-scoped bookmark deletion"
elif rg -q 'database\.saveBookmarks\(bookmarks\)' "$android_browser_data_repository_file"; then
  print_failure "Android tab-derived bookmark persistence full-replaces browser bookmarks"
elif ! rg -q 'staleTabBookmarkIds' "$android_browser_data_repository_file" ||
  ! rg -q 'upsertBookmarkItem' "$android_browser_data_repository_file" ||
  ! rg -q 'deleteBookmarkById\(bookmarkId\)' "$android_browser_data_repository_file"; then
  print_failure "Android tab-derived bookmark persistence does not use id-scoped upsert/delete"
elif ! rg -q 'saveSession' "$android_browser_data_repository_file"; then
  print_failure "Android browser data repository is missing session browser storage calls"
elif ! rg -q 'loadRecentHistory' "$android_browser_data_repository_file"; then
  print_failure "Android browser data repository is missing shared browser history readback"
elif ! rg -q 'loadBookmarks' "$android_browser_data_repository_file"; then
  print_failure "Android browser data repository is missing shared browser bookmark readback"
elif ! rg -q 'loadSessionActiveList' "$android_browser_data_repository_file"; then
  print_failure "Android browser data repository is missing shared browser session readback"
elif [[ ! -f "$android_browser_data_module_file" ]]; then
  print_failure "Android BrowserDataModule native bridge is missing"
elif ! rg -q 'getRecentHistory' "$android_browser_data_module_file" ||
  ! rg -q 'getBookmarks' "$android_browser_data_module_file" ||
  ! rg -q 'BrowserDataRepository' "$android_browser_data_module_file"; then
  print_failure "Android BrowserDataModule does not expose shared browser data readback"
elif ! rg -q 'BrowserDataModule' apps/android/actro/android/app/src/main/java/io/actrium/actro/core/ActroPackage.kt; then
  print_failure "Android BrowserDataModule is not registered in ActroPackage"
elif ! rg -q 'loadRecentBrowserHistory' packages/actro-runtime/src/system-apps/HistoryApp.tsx; then
  print_failure "RN HistoryApp does not read back shared browser history when native props are absent"
elif ! rg -q 'BrowserDataRepository' apps/android/actro/android/app/src/main/java/io/actrium/actro/MainViewModel.kt ||
  ! rg -q 'persistBrowserPageData' apps/android/actro/android/app/src/main/java/io/actrium/actro/MainViewModel.kt ||
  ! rg -q 'restoreSharedBrowserSessionIfNeeded' apps/android/actro/android/app/src/main/java/io/actrium/actro/MainViewModel.kt ||
  ! rg -q 'BrowserDataRepository' apps/android/actro/android/app/src/main/java/io/actrium/actro/tablet/TabletViewModel.kt ||
  ! rg -q 'persistBrowserPageData' apps/android/actro/android/app/src/main/java/io/actrium/actro/tablet/TabletViewModel.kt ||
  ! rg -q 'restoreSharedBrowserSessionIfNeeded' apps/android/actro/android/app/src/main/java/io/actrium/actro/tablet/TabletViewModel.kt; then
  print_failure "Android ViewModels do not persist browser-page snapshots through BrowserDataRepository"
else
  print_ok "Android browser history/bookmark/session data uses the shared browser storage adapter"
fi

android_app_browser_storage_duplicates="$(
  rg -n 'browser_history_pages|browser_history_visits|browser_bookmarks|browser_sessions|browser_session_tabs' \
    apps/android/actro/android/app/src/main/java/io/actrium/actro \
    --glob '*.kt' \
    --glob '!**/BrowserStorageDatabase.kt' \
    2>/dev/null || true
)"

if [[ -n "$android_app_browser_storage_duplicates" ]]; then
  print_failure "Android app code duplicates browser storage table names instead of using BrowserStorageDatabase"
  printf '%s\n' "$android_app_browser_storage_duplicates"
else
  print_ok "Android app code does not duplicate browser storage table names"
fi

open_tab_route_overextraction="$(
  rg -n 'runtime_open_tab_|runtimeOpenTab(Kind|Plan|SystemAppTypeKey|TabIdKey|TabSystemAppTypeKey)|RuntimeOpenTabPlanInfo' \
    crates/actro-core/src \
    crates/actro-core/generated \
    platform/apple/packages/ActroCore/Sources \
    packages/actro-native-kotlin/src/main/kotlin/uniffi \
    packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt \
    2>/dev/null || true
)"

if [[ -n "$open_tab_route_overextraction" ]]; then
  print_failure "openTab UI routing leaked back into Rust/generated bindings"
  printf '%s\n' "$open_tab_route_overextraction"
else
  print_ok "openTab UI routing stays local to native bridge adapters"
fi

macos_tab_file="apps/macos/actro/Actro/Models/Tab.swift"
macos_tab_runtime_bridge_file="apps/macos/actro/Actro/Models/TabRuntimeBridge.swift"
macos_local_fingerprint_impl="$(
  rg -n 'import CryptoKit|SHA256\.hash|SHA256\(' "$macos_tab_file" "$macos_tab_runtime_bridge_file" 2>/dev/null || true
)"

if [[ -n "$macos_local_fingerprint_impl" ]]; then
  print_failure "macOS tab bundle fingerprint reimplements hashing outside ActroCore"
  printf '%s\n' "$macos_local_fingerprint_impl"
elif [[ ! -f "$macos_tab_runtime_bridge_file" ]]; then
  print_failure "macOS tab runtime bridge adapter is missing"
elif ! rg -q 'stableContentFingerprint\(input: self\)' "$macos_tab_runtime_bridge_file"; then
  print_failure "macOS tab bundle fingerprint does not delegate to ActroCore"
else
  print_ok "macOS tab bundle fingerprint delegates to ActroCore"
fi

if [[ -f "$macos_tab_runtime_bridge_file" ]] && rg -q 'import ActroCore' "$macos_tab_runtime_bridge_file"; then
  print_ok "macOS tab runtime bridge adapter owns ActroCore runtime conversion"
else
  print_failure "macOS tab runtime bridge adapter does not own the ActroCore import"
fi

macos_tab_model_rust_imports="$(
  rg -n 'import ActroCore|stableContentFingerprint|runtimeBridge(TabId|Title|ContainerWidth|ContainerHeight)Key|runtimeOpenTabSystemAppTypeKey|enum RuntimeBridgeContract' \
    "$macos_tab_file" 2>/dev/null || true
)"

if [[ -n "$macos_tab_model_rust_imports" ]]; then
  print_failure "macOS Tab model depends on ActroCore runtime bridge helpers instead of TabRuntimeBridge"
  printf '%s\n' "$macos_tab_model_rust_imports"
else
  print_ok "macOS Tab model delegates ActroCore runtime bridge helpers to TabRuntimeBridge"
fi

ios_dead_sha256_impl="$(
  rg -n 'import CommonCrypto|CC_SHA256|var sha256: String|CryptoJS\.SHA256' \
    apps/ios/actro/Actro/Extension/String+Extension.swift \
    2>/dev/null || true
)"

if [[ -n "$ios_dead_sha256_impl" ]]; then
  print_failure "iOS string utilities reintroduce an app-local SHA-256 helper"
  printf '%s\n' "$ios_dead_sha256_impl"
else
  print_ok "iOS string utilities avoid app-local SHA-256 helpers"
fi

ios_tab_runtime_bridge_file="apps/ios/actro/Actro/Models/TabRuntimeBridge.swift"
if [[ ! -f "$ios_tab_runtime_bridge_file" ]]; then
  print_failure "iOS tab runtime bridge adapter is missing"
elif ! rg -q 'import ActroCore' "$ios_tab_runtime_bridge_file"; then
  print_failure "iOS tab runtime bridge adapter does not own the ActroCore import"
else
  print_ok "iOS tab runtime bridge adapter owns ActroCore runtime conversion"
fi

ios_tab_model_rust_imports="$(
  rg -n 'import ActroCore|RuntimeTabInfo|RuntimeRootPropsInfo|runtimeRootPropsFromRuntimeTabs|runtimeTab(Update|Bookmark|Fullscreen)|normalizeRuntimeAppStatus' \
    apps/ios/actro/Actro/Models/Tab.swift \
    2>/dev/null || true
)"

if [[ -n "$ios_tab_model_rust_imports" ]]; then
  print_failure "iOS Tab model depends on Rust-generated runtime DTOs instead of TabRuntimeBridge"
  printf '%s\n' "$ios_tab_model_rust_imports"
else
  print_ok "iOS Tab model delegates Rust runtime DTO conversion to TabRuntimeBridge"
fi

apple_model_core_bridge_requirements=(
  "apps/ios/actro/Actro/Models/ActroAppIdentityCoreBridge.swift|extractAppIdFromUrl|iOS ActroAppIdentityCoreBridge owns core-backed app id extraction"
  "apps/ios/actro/Actro/Models/TagCoreBridge.swift|defaultSystemTags|iOS TagCoreBridge owns core-backed default tags"
  "apps/macos/actro/Actro/Models/ActroAppIdentityCoreBridge.swift|extractAppIdFromUrl|macOS ActroAppIdentityCoreBridge owns core-backed app id extraction"
  "apps/macos/actro/Actro/Models/BrowserSearchEngineCoreBridge.swift|browserSearchUrl|macOS BrowserSearchEngineCoreBridge owns core-backed browser search engine policy"
  "apps/macos/actro/Actro/Models/TagCoreBridge.swift|defaultSystemTags|macOS TagCoreBridge owns core-backed default tags"
  "apps/macos/actro/Actro/Models/InstalledActroAppCoreBridge.swift|parseAppSourceType|macOS InstalledActroAppCoreBridge owns core-backed source type parsing"
  "apps/ios/actro/Actro/App/RuntimeBridgeContracts.swift|userInfoBridgeContract|iOS RuntimeBridgeContracts owns user info bridge contract"
  "apps/ios/actro/Actro/App/RuntimeBridgeContracts.swift|ttsBridgeContract|iOS RuntimeBridgeContracts owns TTS bridge contract"
  "apps/ios/actro/Actro/App/RuntimeBridgeContracts.swift|AuthCoreBridge|iOS RuntimeBridgeContracts owns auth bridge helpers"
)

for requirement in "${apple_model_core_bridge_requirements[@]}"; do
  bridge_file="${requirement%%|*}"
  remainder="${requirement#*|}"
  required_pattern="${remainder%%|*}"
  bridge_description="${remainder##*|}"

  if [[ ! -f "$bridge_file" ]]; then
    print_failure "$bridge_description is missing"
  elif ! rg -q 'import ActroCore' "$bridge_file"; then
    print_failure "$bridge_description does not import ActroCore"
  elif ! rg -q "$required_pattern" "$bridge_file"; then
    print_failure "$bridge_description does not call the expected shared helper"
  else
    print_ok "$bridge_description"
  fi
done

apple_pure_model_core_imports="$(
  rg -n 'import ActroCore|DefaultTagInfo|defaultTagColors|defaultSystemTags|parseAppSourceType|appSourceTypeInfo|CoreInstalledAppInfo|CoreInstalledAppRowInfo|BrowserSearchEngineInfo|browserSearchEngine|browserSearchUrl' \
    apps/ios/actro/Actro/Models/Tag.swift \
    apps/macos/actro/Actro/Models/Tag.swift \
    apps/macos/actro/Actro/Models/InstalledActroApp.swift \
    apps/macos/actro/Actro/Settings/SearchEngineSettings.swift \
    2>/dev/null || true
)"

if [[ -n "$apple_pure_model_core_imports" ]]; then
  print_failure "Apple pure model files depend on ActroCore instead of model CoreBridge adapters"
  printf '%s\n' "$apple_pure_model_core_imports"
else
  print_ok "Apple pure model files delegate shared rules to model CoreBridge adapters"
fi

ios_auth_user_tts_boundary_imports="$(
  rg -n 'import ActroCore|userInfoBridgeContract\(|ttsBridgeContract\(|runtime(LoginResult|AuthContext)|authEnvironment(Dev|Beta|Prod|FromUrl|FromAppUrl|StorageSuffix)\(|authAccessControlUrlFromAppUrl\(|authIsAuthorizedCode\(' \
    apps/ios/actro/Actro/Manager/LoginManager.swift \
    apps/ios/actro/Actro/Manager/UserInfoManager.swift \
    apps/ios/actro/Actro/App/UserInfoModule.swift \
    apps/ios/actro/Actro/App/TTSModule.swift \
    2>/dev/null || true
)"

if [[ -n "$ios_auth_user_tts_boundary_imports" ]]; then
  print_failure "iOS auth/user/TTS modules bypass RuntimeBridgeContracts"
  printf '%s\n' "$ios_auth_user_tts_boundary_imports"
else
  print_ok "iOS auth/user/TTS modules delegate shared wire contracts to RuntimeBridgeContracts"
fi

apple_allowed_actrocore_import_files=(
  "apps/ios/actro/Actro/ActrBridge/ActrBridge.swift"
  "apps/ios/actro/Actro/App/RuntimeBridgeContracts.swift"
  "apps/ios/actro/Actro/Database/BrowserDataRepository.swift"
  "apps/ios/actro/Actro/Database/BrowserSyncCredentialsProvider.swift"
  "apps/ios/actro/Actro/Database/DatabaseManager.swift"
  "apps/ios/actro/Actro/Database/HistoryRepository.swift"
  "apps/ios/actro/Actro/Database/TagRepository.swift"
  "apps/ios/actro/Actro/Models/ActroAppIdentityCoreBridge.swift"
  "apps/ios/actro/Actro/Models/ActrPackageManifestReader.swift"
  "apps/ios/actro/Actro/Models/TabRuntimeBridge.swift"
  "apps/ios/actro/Actro/Models/TagCoreBridge.swift"
  "apps/macos/actro/Actro/Database/AppRepository.swift"
  "apps/macos/actro/Actro/Database/BrowserDataRepository.swift"
  "apps/macos/actro/Actro/Database/BookmarkRepository.swift"
  "apps/macos/actro/Actro/Database/DatabaseManager.swift"
  "apps/macos/actro/Actro/Database/HistoryRepository.swift"
  "apps/macos/actro/Actro/Database/TagRepository.swift"
  "apps/macos/actro/Actro/Models/ActroAppIdentityCoreBridge.swift"
  "apps/macos/actro/Actro/Models/ActrPackageManifestReader.swift"
  "apps/macos/actro/Actro/Models/BrowserSearchEngineCoreBridge.swift"
  "apps/macos/actro/Actro/Models/InstalledActroAppCoreBridge.swift"
  "apps/macos/actro/Actro/Models/TabRuntimeBridge.swift"
  "apps/macos/actro/Actro/Models/TagCoreBridge.swift"
  "apps/macos/actro/Actro/Settings/Sync/BrowserSyncCredentialsProvider.swift"
  "apps/macos/actro/Actro/Settings/Sync/BrowserSyncProviderConnectionPlanner.swift"
)

apple_direct_actrocore_import_files="$(
  rg -l '^import ActroCore' \
    apps/ios/actro/Actro \
    apps/macos/actro/Actro \
    --glob '*.swift' \
    2>/dev/null | sort || true
)"

apple_unexpected_actrocore_imports="$(
  while IFS= read -r file; do
    [[ -z "$file" ]] && continue

    is_allowed=false
    for allowed_file in "${apple_allowed_actrocore_import_files[@]}"; do
      if [[ "$file" == "$allowed_file" ]]; then
        is_allowed=true
        break
      fi
    done

    if [[ "$is_allowed" == false ]]; then
      echo "$file"
    fi
  done <<< "$apple_direct_actrocore_import_files"
)"

if [[ -n "$apple_unexpected_actrocore_imports" ]]; then
  print_failure "Apple app ActroCore imports escaped the adapter/repository allowlist"
  printf '%s\n' "$apple_unexpected_actrocore_imports"
else
  print_ok "Apple app ActroCore imports stay confined to adapter/repository boundary files"
fi

runtime_dead_sha256_impl="$(
  rg -n 'CryptoJS|Utils\.sha256|static sha256|readFileAsBinary|fetch\(guestUrl\)|arrayBuffer\(\)' \
    packages/actro-runtime/src/services/AppSessionService.ts \
    packages/actro-runtime/src/utils/utils.ts \
    2>/dev/null || true
)"

if [[ -n "$runtime_dead_sha256_impl" ]]; then
  print_failure "RN runtime reintroduces the old JS .actr SHA-256 helper path"
  printf '%s\n' "$runtime_dead_sha256_impl"
else
  print_ok "RN runtime avoids the old JS .actr SHA-256 helper path"
fi

runtime_hash_dependency_files=(
  "packages/actro-runtime/package.json"
  "packages/actro-runtime/yarn.lock"
  "packages/actro-runtime/bun.lock"
  "apps/android/actro/package.json"
  "apps/android/actro/yarn.lock"
)

runtime_hash_dependency_residue="$(
  rg -n 'crypto-js|@types/crypto-js' \
    "${runtime_hash_dependency_files[@]}" \
    2>/dev/null || true
)"

if [[ -n "$runtime_hash_dependency_residue" ]]; then
  print_failure "runtime package manifests/locks reintroduce unused CryptoJS dependencies"
  printf '%s\n' "$runtime_hash_dependency_residue"
else
  print_ok "runtime package manifests/locks avoid unused CryptoJS dependencies"
fi

if ! rg -q 'expectedSha256: guestHashCode' \
  packages/actro-runtime/src/services/AppSessionService.ts; then
  print_failure "RN .actr session loading does not pass the server hash into ActrLoader"
else
  print_ok "RN .actr session loading verifies the downloaded archive in ActrLoader"
fi

# Windows OS-level interop (dwmapi window chrome) is not Rust-core access:
# only the actro_core boundary must stay centralized in ActroCore.cs.
windows_native_imports="$(
  rg -n 'DllImport\(|LibraryImport\(|actro_core_' apps/windows/actro/src \
    --glob '!ActroCore.cs' 2>/dev/null \
    | rg -v 'DllImport\("dwmapi\.dll"' \
    || true
)"

if [[ -n "$windows_native_imports" ]]; then
  print_failure "Windows UI files bypass the centralized ActroCore adapter"
  printf '%s\n' "$windows_native_imports"
else
  print_ok "Windows native core access is centralized in ActroCore.cs"
fi

windows_browser_data_requirements=(
  "apps/windows/actro/src/BrowserHistoryStore.cs|ActroCore.NormalizeBrowserHistorySnapshotJson|Windows BrowserHistoryStore delegates snapshot schema normalization to ActroCore"
  "apps/windows/actro/src/BrowserBookmarkStore.cs|ActroCore.NormalizeBrowserBookmarkSnapshotJson|Windows BrowserBookmarkStore delegates snapshot schema normalization to ActroCore"
  "apps/windows/actro/src/BrowserSessionStore.cs|ActroCore.NormalizeBrowserSessionSnapshotJson|Windows BrowserSessionStore delegates snapshot schema normalization to ActroCore"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_history_snapshot_normalize_json|Windows ActroCore adapter exposes browser data C ABI"
  "apps/windows/actro/src/ActroCore.cs|LoadBrowserStorageSchema|Windows ActroCore adapter exposes browser storage schema"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_schema_statements_json|Windows ActroCore adapter exposes browser storage schema C ABI"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_sql_json|Windows ActroCore adapter exposes browser storage sync SQL C ABI"
  "apps/windows/actro/src/ActroCore.cs|LoadBrowserStorageDataModelJson|Windows ActroCore adapter exposes browser storage data model"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_data_model_json|Windows ActroCore adapter exposes browser storage data model C ABI"
  "apps/windows/actro/src/ActroCore.cs|LoadBrowserStorageSyncConflictPolicyJson|Windows ActroCore adapter exposes browser sync conflict policy"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_conflict_policy_json|Windows ActroCore adapter exposes browser sync conflict policy C ABI"
  "apps/windows/actro/src/ActroCore.cs|LoadBrowserStorageSyncMergePolicyJson|Windows ActroCore adapter exposes browser sync merge policy"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_merge_policy_json|Windows ActroCore adapter exposes browser sync merge policy C ABI"
  "apps/windows/actro/src/ActroCore.cs|LoadBrowserStorageAddressCanonicalizationPolicyJson|Windows ActroCore adapter exposes browser storage address canonicalization policy"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_address_canonicalization_policy_json|Windows ActroCore adapter exposes browser storage address canonicalization policy C ABI"
  "apps/windows/actro/src/ActroCore.cs|CanonicalizeBrowserStorageAddressesJson|Windows ActroCore adapter exposes browser storage address canonicalization"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_canonicalize_addresses_json|Windows ActroCore adapter exposes browser storage address canonicalization C ABI"
  "apps/windows/actro/src/ActroCore.cs|LoadBrowserStorageBookmarkFolderTreeIntegrityPolicyJson|Windows ActroCore adapter exposes bookmark folder tree integrity policy"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_bookmark_folder_tree_integrity_policy_json|Windows ActroCore adapter exposes bookmark folder tree integrity policy C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageBookmarkFolderTreeIntegrityPlanJson|Windows ActroCore adapter exposes bookmark folder tree integrity planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_bookmark_folder_tree_integrity_plan_json|Windows ActroCore adapter exposes bookmark folder tree integrity plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|LoadBrowserStorageBookmarkRestorePlacementPolicyJson|Windows ActroCore adapter exposes bookmark restore placement policy"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_bookmark_restore_placement_policy_json|Windows ActroCore adapter exposes bookmark restore placement policy C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageBookmarkRestorePlacementPlanJson|Windows ActroCore adapter exposes bookmark restore placement planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_bookmark_restore_placement_plan_json|Windows ActroCore adapter exposes bookmark restore placement plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|LoadBrowserStorageBookmarkOwnershipPolicyJson|Windows ActroCore adapter exposes bookmark ownership policy"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_bookmark_ownership_policy_json|Windows ActroCore adapter exposes bookmark ownership policy C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageBookmarkOwnershipPlanJson|Windows ActroCore adapter exposes bookmark ownership planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_bookmark_ownership_plan_json|Windows ActroCore adapter exposes bookmark ownership plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|LoadBrowserStorageHistoryVisitAggregationPolicyJson|Windows ActroCore adapter exposes history visit aggregation policy"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_history_visit_aggregation_policy_json|Windows ActroCore adapter exposes history visit aggregation policy C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageHistoryVisitAggregationPlanJson|Windows ActroCore adapter exposes history visit aggregation planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_history_visit_aggregation_plan_json|Windows ActroCore adapter exposes history visit aggregation plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|LoadBrowserStorageSessionTabOrderingPolicyJson|Windows ActroCore adapter exposes session tab ordering policy"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_session_tab_ordering_policy_json|Windows ActroCore adapter exposes session tab ordering policy C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSessionTabOrderingPlanJson|Windows ActroCore adapter exposes session tab ordering planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_session_tab_ordering_plan_json|Windows ActroCore adapter exposes session tab ordering plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSessionTabIdPlanJson|Windows ActroCore adapter exposes session tab id planning"
  "apps/windows/actro/src/ActroCore.cs|LoadBrowserStorageSessionActiveTabConsistencyPolicyJson|Windows ActroCore adapter exposes session active-tab consistency policy"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_session_active_tab_consistency_policy_json|Windows ActroCore adapter exposes session active-tab consistency policy C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSessionActiveTabConsistencyPlanJson|Windows ActroCore adapter exposes session active-tab consistency planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_session_active_tab_consistency_plan_json|Windows ActroCore adapter exposes session active-tab consistency plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|LoadBrowserStorageRetentionPruningPolicyJson|Windows ActroCore adapter exposes retention pruning policy"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_retention_pruning_policy_json|Windows ActroCore adapter exposes retention pruning policy C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageRetentionPruningPlanJson|Windows ActroCore adapter exposes retention pruning planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_retention_pruning_plan_json|Windows ActroCore adapter exposes retention pruning plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|NormalizeBrowserStorageSyncBatchJson|Windows ActroCore adapter exposes browser sync batch normalization"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_batch_normalize_json|Windows ActroCore adapter exposes browser sync batch C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncBatchFromDirtyRowsJson|Windows ActroCore adapter exposes dirty-row batch building"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_batch_from_dirty_rows_json|Windows ActroCore adapter exposes dirty-row batch C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncPushRequestFromDirtyRowsJson|Windows ActroCore adapter exposes dirty-row push request building"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_push_request_from_dirty_rows_json|Windows ActroCore adapter exposes dirty-row push request C ABI"
  "apps/windows/actro/src/ActroCore.cs|NormalizeBrowserStorageSyncAckBatchJson|Windows ActroCore adapter exposes browser sync ack normalization"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_ack_batch_normalize_json|Windows ActroCore adapter exposes browser sync ack batch C ABI"
  "apps/windows/actro/src/ActroCore.cs|NormalizeBrowserStorageSyncCursorsJson|Windows ActroCore adapter exposes browser sync cursor normalization"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cursors_normalize_json|Windows ActroCore adapter exposes browser sync cursor C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncPushRequestJson|Windows ActroCore adapter exposes browser sync push request building"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_push_request_json|Windows ActroCore adapter exposes browser sync push request C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncPullRequestJson|Windows ActroCore adapter exposes browser sync pull request building"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_pull_request_json|Windows ActroCore adapter exposes browser sync pull request C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCycleResponseJson|Windows ActroCore adapter exposes browser sync cycle response building"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cycle_response_json|Windows ActroCore adapter exposes browser sync cycle response C ABI"
  "apps/windows/actro/src/ActroCore.cs|NormalizeBrowserStorageSyncCycleResponseJson|Windows ActroCore adapter exposes browser sync cycle response normalization"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cycle_response_normalize_json|Windows ActroCore adapter exposes browser sync cycle response normalization C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncClientCyclePlanJson|Windows ActroCore adapter exposes browser sync client cycle planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_client_cycle_plan_json|Windows ActroCore adapter exposes browser sync client cycle plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncClientAckBatchAfterRemoteApplyJson|Windows ActroCore adapter exposes browser sync post-remote ack selection"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_client_ack_batch_after_remote_apply_json|Windows ActroCore adapter exposes browser sync post-remote ack C ABI"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_cycle_client_apply_result_json|Windows ActroCore adapter exposes cloud cycle apply result C ABI"
  "apps/windows/actro/src/ActroCore.cs|LoadBrowserStorageSyncTransportContractJson|Windows ActroCore adapter exposes browser sync transport contract"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_transport_contract_json|Windows ActroCore adapter exposes browser sync transport contract C ABI"
  "apps/windows/actro/src/ActroCore.cs|LoadBrowserStorageSyncCloudStoreContractJson|Windows ActroCore adapter exposes browser sync cloud store contract"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_store_contract_json|Windows ActroCore adapter exposes browser sync cloud store contract C ABI"
  "apps/windows/actro/src/ActroCore.cs|LoadBrowserStorageSyncCloudProviderAdapterContractJson|Windows ActroCore adapter exposes browser sync cloud provider adapter contract"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_provider_adapter_contract_json|Windows ActroCore adapter exposes browser sync cloud provider adapter contract C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudProviderAdapterPlanJson|Windows ActroCore adapter exposes browser sync cloud provider adapter planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_provider_adapter_plan_json|Windows ActroCore adapter exposes browser sync cloud provider adapter plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|LoadBrowserStorageSyncCloudProviderConnectionContractJson|Windows ActroCore adapter exposes browser sync cloud provider connection contract"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_provider_connection_contract_json|Windows ActroCore adapter exposes browser sync cloud provider connection contract C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudProviderConnectionStatusPolicyJson|Windows ActroCore adapter exposes browser sync cloud provider connection status policy"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_provider_connection_status_policy_json|Windows ActroCore adapter exposes browser sync cloud provider connection status policy C ABI"
  "apps/windows/actro/src/ActroCore.cs|LoadBrowserStorageSyncCloudProviderErrorPolicyJson|Windows ActroCore adapter exposes browser sync cloud provider error policy"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudProviderErrorClassificationJson|Windows ActroCore adapter exposes browser sync cloud provider error classification"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_provider_error_classification_json|Windows ActroCore adapter exposes browser sync cloud provider error classifier C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudStoreCapabilityPreflightJson|Windows ActroCore adapter exposes browser sync cloud store capability preflight"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_store_capability_preflight_json|Windows ActroCore adapter exposes browser sync cloud store capability preflight C ABI"
  "apps/windows/actro/src/ActroCore.cs|LoadBrowserStorageSyncCloudLocalFolderCapabilitiesJson|Windows ActroCore adapter exposes browser sync local-folder capabilities"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_local_folder_capabilities_json|Windows ActroCore adapter exposes browser sync local-folder capabilities C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudLocalFolderCapabilityPreflightJson|Windows ActroCore adapter exposes browser sync local-folder capability preflight"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_local_folder_capability_preflight_json|Windows ActroCore adapter exposes browser sync local-folder capability preflight C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudLocalFolderCompatibilityPreflightJson|Windows ActroCore adapter exposes browser sync local-folder compatibility preflight"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_local_folder_compatibility_preflight_json|Windows ActroCore adapter exposes browser sync local-folder compatibility preflight C ABI"
  "apps/windows/actro/src/ActroCore.cs|LoadBrowserStorageSyncCloudDataModelJson|Windows ActroCore adapter exposes browser sync cloud data model"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_data_model_json|Windows ActroCore adapter exposes browser sync cloud data model C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudCompatibilityStatusJson|Windows ActroCore adapter exposes browser sync cloud compatibility status"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_compatibility_status_json|Windows ActroCore adapter exposes browser sync cloud compatibility status C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudCompatibilityPreflightJson|Windows ActroCore adapter exposes browser sync cloud compatibility preflight"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_compatibility_preflight_json|Windows ActroCore adapter exposes browser sync cloud compatibility preflight C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudSchemaRolloutPlanJson|Windows ActroCore adapter exposes browser sync cloud schema rollout planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_schema_rollout_plan_json|Windows ActroCore adapter exposes browser sync cloud schema rollout plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudSchemaMigrationPlaybookJson|Windows ActroCore adapter exposes browser sync cloud schema migration playbook"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_schema_migration_playbook_json|Windows ActroCore adapter exposes browser sync cloud schema migration playbook C ABI"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_schema_migration_plan_json|Windows ActroCore adapter exposes the browser storage schema migration plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudRecoverySnapshotPlanJson|Windows ActroCore adapter exposes browser sync cloud recovery snapshot planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_recovery_snapshot_plan_json|Windows ActroCore adapter exposes browser sync cloud recovery snapshot plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudRecoverySnapshotResultPlanJson|Windows ActroCore adapter exposes browser sync cloud recovery snapshot result handoff"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_recovery_snapshot_result_plan_json|Windows ActroCore adapter exposes browser sync cloud recovery snapshot result C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudLocalFolderRecoverySnapshotRoundJson|Windows ActroCore adapter exposes browser sync local-folder recovery snapshot round"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_local_folder_recovery_snapshot_round_json|Windows ActroCore adapter exposes browser sync local-folder recovery snapshot round C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudDefaultManifestJson|Windows ActroCore adapter exposes browser sync cloud manifest defaults"
  "apps/windows/actro/src/ActroCore.cs|NormalizeBrowserStorageSyncCloudManifestJson|Windows ActroCore adapter exposes browser sync cloud manifest normalization"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudLogObjectJson|Windows ActroCore adapter exposes browser sync cloud log objects"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudSnapshotObjectJson|Windows ActroCore adapter exposes browser sync cloud snapshot objects"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudMembershipPlanJson|Windows ActroCore adapter exposes browser sync cloud membership planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_membership_plan_json|Windows ActroCore adapter exposes browser sync cloud membership plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudMembershipResultPlanJson|Windows ActroCore adapter exposes browser sync cloud membership result planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_membership_result_plan_json|Windows ActroCore adapter exposes browser sync cloud membership result plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudPushPlanJson|Windows ActroCore adapter exposes browser sync cloud push planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_push_plan_json|Windows ActroCore adapter exposes browser sync cloud push plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudPushResultPlanJson|Windows ActroCore adapter exposes browser sync cloud push result planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_push_result_plan_json|Windows ActroCore adapter exposes browser sync cloud push result plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudPullPlanJson|Windows ActroCore adapter exposes browser sync cloud pull planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_pull_plan_json|Windows ActroCore adapter exposes browser sync cloud pull plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudPullApplyPlanJson|Windows ActroCore adapter exposes browser sync cloud pull apply planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_pull_apply_plan_json|Windows ActroCore adapter exposes browser sync cloud pull apply plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudPullContinuationPlanJson|Windows ActroCore adapter exposes browser sync cloud pull continuation planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_pull_continuation_plan_json|Windows ActroCore adapter exposes browser sync cloud pull continuation plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudCompactionPlanJson|Windows ActroCore adapter exposes browser sync cloud compaction planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_compaction_plan_json|Windows ActroCore adapter exposes browser sync cloud compaction plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudCompactionResultPlanJson|Windows ActroCore adapter exposes browser sync cloud compaction result planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_compaction_result_plan_json|Windows ActroCore adapter exposes browser sync cloud compaction result plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|RunBrowserStorageSyncCloudLocalFolderCompactionRoundJson|Windows ActroCore adapter exposes browser sync local-folder compaction round"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_local_folder_compaction_round_json|Windows ActroCore adapter exposes browser sync local-folder compaction round C ABI"
  "apps/windows/actro/src/ActroCore.cs|RunBrowserStorageSyncCloudLocalFolderCleanupRoundJson|Windows ActroCore adapter exposes browser sync local-folder cleanup round"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_local_folder_cleanup_round_json|Windows ActroCore adapter exposes browser sync local-folder cleanup round C ABI"
  "apps/windows/actro/src/ActroCore.cs|LoadBrowserStorageSyncCloudHighWatermarkPolicyJson|Windows ActroCore adapter exposes browser sync cloud high-watermark policy"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_high_watermark_policy_json|Windows ActroCore adapter exposes browser sync cloud high-watermark policy C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudHighWatermarkPlanJson|Windows ActroCore adapter exposes browser sync cloud high-watermark planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_high_watermark_plan_json|Windows ActroCore adapter exposes browser sync cloud high-watermark plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudCleanupResultPlanJson|Windows ActroCore adapter exposes browser sync cloud cleanup result planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_cleanup_result_plan_json|Windows ActroCore adapter exposes browser sync cloud cleanup result plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudCycleClientPlanJson|Windows ActroCore adapter exposes browser sync cloud cycle client planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_cycle_client_plan_json|Windows ActroCore adapter exposes browser sync cloud cycle client plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudCycleStatusPolicyJson|Windows ActroCore adapter exposes browser sync cloud cycle status policy"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_cycle_status_policy_json|Windows ActroCore adapter exposes browser sync cloud cycle status policy C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudMembershipStatusPolicyJson|Windows ActroCore adapter exposes browser sync cloud membership status policy"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_membership_status_policy_json|Windows ActroCore adapter exposes browser sync cloud membership status policy C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudCycleScheduleStateJson|Windows ActroCore adapter exposes browser sync cloud cycle schedule state"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_cycle_schedule_state_json|Windows ActroCore adapter exposes browser sync cloud cycle schedule state C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudMembershipScheduleStateJson|Windows ActroCore adapter exposes browser sync cloud membership schedule state"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_membership_schedule_state_json|Windows ActroCore adapter exposes browser sync cloud membership schedule state C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudMembershipAttemptPlanJson|Windows ActroCore adapter exposes browser sync cloud membership attempt plan"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_membership_attempt_plan_json|Windows ActroCore adapter exposes browser sync cloud membership attempt plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudMembershipLoopPlanJson|Windows ActroCore adapter exposes browser sync cloud membership loop plan"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_membership_loop_plan_json|Windows ActroCore adapter exposes browser sync cloud membership loop plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudProviderDiagnosticsJson|Windows ActroCore adapter exposes browser sync cloud provider diagnostics"
  "apps/windows/actro/src/ActroCore.cs|providerErrorClassificationJson|Windows ActroCore diagnostics adapter accepts provider error classification"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_provider_diagnostics_json|Windows ActroCore adapter exposes browser sync cloud provider diagnostics C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudObservabilityEventJson|Windows ActroCore adapter exposes browser sync cloud observability events"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_observability_event_json|Windows ActroCore adapter exposes browser sync cloud observability event C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudCycleAttemptPlanJson|Windows ActroCore adapter exposes browser sync cloud cycle attempt planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_cycle_attempt_plan_json|Windows ActroCore adapter exposes browser sync cloud cycle attempt plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudCycleLoopPlanJson|Windows ActroCore adapter exposes browser sync cloud cycle loop planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_cycle_loop_plan_json|Windows ActroCore adapter exposes browser sync cloud cycle loop plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCloudLocalFolderCycleRoundJson|Windows ActroCore adapter exposes browser sync local-folder cycle rounds"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_local_folder_cycle_round_json|Windows ActroCore adapter exposes browser sync local-folder cycle round C ABI"
  "apps/windows/actro/src/ActroCore.cs|RunBrowserStorageSyncCloudLocalFolderMembershipRoundJson|Windows ActroCore adapter exposes browser sync local-folder membership rounds"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cloud_local_folder_membership_round_json|Windows ActroCore adapter exposes browser sync local-folder membership round C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCycleRequestPlanJson|Windows ActroCore adapter exposes browser sync cycle request planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cycle_request_plan_json|Windows ActroCore adapter exposes browser sync cycle request plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncCycleTransportResultJson|Windows ActroCore adapter exposes browser sync transport result classification"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_cycle_transport_result_json|Windows ActroCore adapter exposes browser sync transport result C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncStatusPolicyJson|Windows ActroCore adapter exposes browser sync status policy"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_status_policy_json|Windows ActroCore adapter exposes browser sync status policy C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncScheduleStateJson|Windows ActroCore adapter exposes browser sync schedule state"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_schedule_state_json|Windows ActroCore adapter exposes browser sync schedule state C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncSchedulerStatusJson|Windows ActroCore adapter exposes browser sync scheduler status"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_scheduler_status_json|Windows ActroCore adapter exposes browser sync scheduler status C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncAckPlanJson|Windows ActroCore adapter exposes browser sync ack planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_ack_plan_json|Windows ActroCore adapter exposes browser sync ack plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncRemoteApplyPlanJson|Windows ActroCore adapter exposes browser sync remote apply planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_remote_apply_plan_json|Windows ActroCore adapter exposes browser sync remote apply C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncRemoteApplyResultJson|Windows ActroCore adapter exposes browser sync remote apply result summaries"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_remote_apply_result_json|Windows ActroCore adapter exposes browser sync remote apply result C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncServerMergePlanJson|Windows ActroCore adapter exposes browser sync reference merge planning"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_server_merge_plan_json|Windows ActroCore adapter exposes browser sync reference merge plan C ABI"
  "apps/windows/actro/src/ActroCore.cs|BuildBrowserStorageSyncServerMergeResultJson|Windows ActroCore adapter exposes browser sync reference merge result summaries"
  "apps/windows/actro/src/ActroCore.cs|actro_core_browser_storage_sync_server_merge_result_json|Windows ActroCore adapter exposes browser sync reference merge result C ABI"
  "apps/windows/actro/src/ActroCore.cs|LoadAuthEnvironmentProd|Windows ActroCore adapter exposes Rust auth environment defaults"
  "apps/windows/actro/src/ActroCore.cs|actro_core_auth_account_base_url|Windows ActroCore adapter exposes Rust auth endpoint C ABI"
  "apps/windows/actro/src/BrowserStorageDatabase.cs|ActroCore.LoadBrowserStorageSchema|Windows browser storage adapter consumes Rust storage schema"
  "apps/windows/actro/src/BrowserStorageDatabase.cs|LoadBrowserStorageSchemaMigrationPlanJson|Windows browser storage adapter consumes the Rust schema migration plan"
  "apps/windows/actro/src/BrowserStorageDatabase.cs|Microsoft.Data.Sqlite|Windows browser storage adapter owns SQLite execution"
  "apps/windows/actro/src/BrowserStorageDatabase.cs|browser_history_pages|Windows browser storage adapter targets Rust-owned browser storage tables"
  "apps/windows/actro/src/BrowserStorageDatabase.cs|tab_id|Windows browser storage adapter persists stable browser session tab ids"
  "apps/windows/actro/src/BrowserStorageDatabase.cs|BuildBrowserStorageSessionTabIdPlanJson|Windows browser storage adapter derives session tab ids through the Rust batch plan"
  "apps/windows/actro/src/BrowserStorageDatabase.cs|deleted_at|Windows browser storage adapter preserves browser tombstones"
  "apps/windows/actro/src/BrowserStorageDatabase.cs|browser_sync_cursors|Windows browser storage adapter persists sync cursors"
  "apps/windows/actro/src/BrowserStorageDatabase.cs|ApplyRemoteSyncPlan|Windows browser storage adapter executes Rust browser sync remote apply plans"
  "apps/windows/actro/src/BrowserStorageDatabase.cs|ApplySyncAckPlan|Windows browser storage adapter executes Rust browser sync ack plans"
  "apps/windows/actro/src/BrowserStorageDatabase.cs|ApplySyncCycleResponse|Windows browser storage adapter applies Rust-planned sync cycles"
  "apps/windows/actro/src/BrowserStorageDatabase.cs|ApplyCloudCycleClientPlan|Windows browser storage adapter applies Rust cloud cycle client plans"
  "apps/windows/actro/src/BrowserStorageDatabase.cs|ExportDirtySyncBatch|Windows browser storage adapter exports dirty rows through Rust batch builder"
  "apps/windows/actro/src/BrowserStorageDatabase.cs|ExportDirtySyncPushRequest|Windows browser storage adapter exports dirty rows through Rust push builder"
  "apps/windows/actro/src/BrowserStorageDatabase.cs|ExportSyncCycleRequestPlan|Windows browser storage adapter exports Rust-planned sync cycle requests"
  "apps/windows/actro/src/BrowserStorageDatabase.cs|SelectDirtyStatements|Windows browser storage adapter consumes Rust dirty-row SQL selectors"
  "apps/windows/actro/src/BrowserStorageDatabase.cs|BrowserStorageSyncSqlContract|Windows browser storage adapter consumes Rust browser sync SQL"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|IBrowserStorageSyncTransport|Windows browser sync client uses injectable transport"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|BuildBrowserStorageSyncCycleTransportResultJson|Windows browser sync client classifies transport through Rust"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|RunLocalFolderSyncCycleAsync|Windows browser sync client exposes local-folder cloud cycle execution"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|RunLocalFolderMembershipRoundAsync|Windows browser sync client exposes local-folder cloud membership execution"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|BuildBrowserStorageSyncCloudLocalFolderCapabilityPreflightJson|Windows browser sync client consumes Rust local-folder capability preflight"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|BuildBrowserStorageSyncCloudLocalFolderCompatibilityPreflightJson|Windows browser sync client consumes Rust local-folder compatibility preflight"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|compatibilityPreflight|Windows browser sync client returns Rust local-folder compatibility preflight"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|BuildBrowserStorageSyncCloudLocalFolderCycleRoundJson|Windows browser sync client consumes Rust local-folder cycle helper"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|RunBrowserStorageSyncCloudLocalFolderCompactionRoundJson|Windows browser sync client consumes Rust local-folder compaction helper"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|RunBrowserStorageSyncCloudLocalFolderCleanupRoundJson|Windows browser sync client consumes Rust local-folder cleanup helper"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|BuildCloudSchemaRolloutPlan|Windows browser sync client consumes Rust schema rollout planning on compatibility block"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|BuildCloudSchemaMigrationPlaybook|Windows browser sync client consumes Rust schema migration playbook on compatibility block"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|BuildLocalFolderRecoverySnapshotRound|Windows browser sync client consumes Rust local-folder recovery snapshot round on compatibility block"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|RunBrowserStorageSyncCloudLocalFolderMembershipRoundJson|Windows browser sync client consumes Rust local-folder membership helper"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|BuildCloudMembershipStatusPolicy|Windows browser sync membership client consumes Rust membership status policy"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|\"statusPolicy\"|Windows browser sync membership client returns Rust membership status policy"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|BuildCloudMembershipScheduleState|Windows browser sync membership client consumes Rust membership schedule state"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|\"scheduleState\"|Windows browser sync membership client returns Rust membership schedule state"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|BuildCloudMembershipAttemptPlan|Windows browser sync membership client consumes Rust membership attempt plan"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|\"attemptPlan\"|Windows browser sync membership client returns Rust membership attempt plan"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|BuildCloudMembershipLoopPlan|Windows browser sync membership client consumes Rust membership loop plan"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|\"loopPlan\"|Windows browser sync membership client returns Rust membership loop plan"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|schemaRolloutPlan = BuildCloudSchemaRolloutPlan|Windows browser sync membership client prepares Rust schema rollout on compatibility block"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|observabilityEvent = BuildCloudObservabilityEvent|Windows browser sync membership client exposes Rust observability events"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|BuildBrowserStorageSyncCloudCycleScheduleStateJson|Windows browser sync client consumes Rust cloud cycle schedule state"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|BuildBrowserStorageSyncCloudProviderDiagnosticsJson|Windows browser sync client consumes Rust cloud provider diagnostics"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|BuildLocalFolderProviderErrorClassification|Windows browser sync client builds provider error classification from real local-folder rounds"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|localFolderProviderRoundErrors|Windows browser sync client sources provider errors from local-folder rounds"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|BuildBrowserStorageSyncCloudObservabilityEventJson|Windows browser sync client consumes Rust cloud observability events"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|BuildBrowserStorageSyncCloudCycleAttemptPlanJson|Windows browser sync client consumes Rust cloud cycle attempt planning"
  "apps/windows/actro/src/BrowserStorageSyncClient.cs|BuildBrowserStorageSyncCloudCycleLoopPlanJson|Windows browser sync client consumes Rust cloud cycle loop planning"
  "apps/windows/actro/src/BrowserStorageSyncCoordinator.cs|IBrowserStorageSyncCredentialsProvider|Windows browser sync coordinator uses injectable credential lookup"
  "apps/windows/actro/src/BrowserStorageSyncCoordinator.cs|BrowserStorageSyncClient|Windows browser sync coordinator delegates network/apply work to the sync client"
  "apps/windows/actro/src/BrowserStorageSyncCoordinator.cs|CreateDefault|Windows browser sync coordinator creates default client and credential store"
  "apps/windows/actro/src/BrowserStorageSyncCoordinator.cs|statusPolicy|Windows browser sync coordinator returns Rust-owned status policy payloads"
  "apps/windows/actro/src/BrowserStorageSyncCoordinator.cs|scheduleState|Windows browser sync coordinator returns Rust-owned schedule state payloads"
  "apps/windows/actro/src/BrowserStorageSyncScheduler.cs|BrowserStorageSyncSchedulerSnapshot|Windows browser sync scheduler exposes platform schedule snapshots"
  "apps/windows/actro/src/BrowserStorageSyncScheduler.cs|NextRunDelaySeconds|Windows browser sync scheduler consumes Rust-owned schedule state timing"
  "apps/windows/actro/src/BrowserStorageSyncScheduler.cs|StatusJson|Windows browser sync scheduler exposes Rust-owned product status"
  "apps/windows/actro/src/BrowserStorageSyncScheduler.cs|ScheduleAfterLocalChange|Windows browser sync scheduler can debounce local browser data changes"
  "apps/windows/actro/src/BrowserMenuModel.cs|ConfigureSync|Windows app menu exposes browser sync credential configuration"
  "apps/windows/actro/src/BrowserMenuModel.cs|SyncNow|Windows app menu exposes manual browser sync"
  "apps/windows/actro/src/MainForm.cs|ConfigureBrowserSyncConnection|Windows main form wires browser sync credential configuration"
  "apps/windows/actro/src/MainForm.cs|SyncBrowserDataNowAsync|Windows main form wires manual browser sync"
  "apps/windows/actro/src/MainForm.cs|ScheduleBrowserDataSyncAfterLocalChange|Windows main form schedules sync after local browser data changes"
  "apps/windows/actro/src/MainForm.cs|ScheduleBrowserDataSyncAfterConnectionChange|Windows main form schedules sync after connection changes"
  "apps/windows/actro/src/BrowserSyncStatusPolicy.cs|FormatStatusJson|Windows UI formats Rust-owned browser sync status locally"
  "apps/windows/actro/src/BrowserStorageSyncCredentialStore.cs|IBrowserStorageSyncCredentialsProvider|Windows browser sync credential store implements coordinator credential lookup"
  "apps/windows/actro/src/BrowserStorageSyncCredentialStore.cs|SaveSession|Windows browser sync credential store persists account sync sessions"
  "apps/windows/actro/src/BrowserStorageSyncCredentialStore.cs|browser-sync-credentials.json|Windows browser sync credential store persists local credential snapshot"
  "apps/windows/actro/src/BrowserSyncConnectionDialog.cs|BrowserSyncConnectionDialog|Windows browser sync configuration dialog exists"
  "apps/windows/actro.tests/Actro.Windows.Tests.csproj|BrowserStorageSyncCoordinator.cs|Windows tests compile the browser sync coordinator"
  "apps/windows/actro.tests/Actro.Windows.Tests.csproj|BrowserStorageSyncCredentialStore.cs|Windows tests compile the browser sync credential store"
  "apps/windows/actro.tests/Actro.Windows.Tests.csproj|BrowserStorageSyncScheduler.cs|Windows tests compile the browser sync scheduler"
  "apps/windows/actro.tests/Actro.Windows.Tests.csproj|BrowserSyncStatusPolicy.cs|Windows tests compile browser sync status UI policy"
  "apps/windows/actro.tests/Program.cs|migrates browser storage sqlite v1 v2 v3 to v6|Windows tests cover browser storage v1/v2/v3 SQLite migration"
  "apps/windows/actro.tests/Program.cs|BrowserSyncBatchHasBookmarkOwner|Windows tests cover browser bookmark owner sync payloads"
  "apps/windows/actro.tests/Program.cs|ApplySyncCycleResponse|Windows tests cover browser sync cycle apply"
  "apps/windows/actro.tests/Program.cs|ExportSyncCycleRequestPlan|Windows tests cover browser sync cycle request planning"
  "apps/windows/actro.tests/Program.cs|runs browser storage sync client cycle|Windows tests cover browser sync client cycle"
  "apps/windows/actro.tests/Program.cs|runs browser storage local-folder cloud sync client cycle|Windows tests cover local-folder cloud sync client"
  "apps/windows/actro.tests/Program.cs|runs browser storage local-folder cloud membership client round|Windows tests cover local-folder cloud membership client"
  "apps/windows/actro.tests/Program.cs|finishCloudDeviceRevoked|Windows local-folder cloud membership test asserts Rust revoke classification"
  "apps/windows/actro.tests/Program.cs|capabilityPreflight|Windows local-folder cloud sync test asserts Rust capability preflight"
  "apps/windows/actro.tests/Program.cs|compatibilityPreflight|Windows local-folder cloud sync test asserts Rust compatibility preflight"
  "apps/windows/actro.tests/Program.cs|clientUpgradeRequired|Windows local-folder cloud sync test asserts compatibility blocking"
  "apps/windows/actro.tests/Program.cs|cloudCycleScheduleState|Windows local-folder cloud sync test asserts Rust schedule state"
  "apps/windows/actro.tests/Program.cs|providerDiagnostics|Windows local-folder cloud sync test asserts Rust provider diagnostics"
  "apps/windows/actro.tests/Program.cs|cloudObservabilityEvent|Windows local-folder cloud sync test asserts Rust observability event"
  "apps/windows/actro.tests/Program.cs|loopPlan|Windows local-folder cloud sync test asserts Rust loop planning"
  "apps/windows/actro.tests/Program.cs|compactionRound|Windows local-folder cloud sync test asserts Rust compaction round"
  "apps/windows/actro.tests/Program.cs|cleanupRound|Windows local-folder cloud sync test asserts Rust cleanup round"
  "apps/windows/actro.tests/Program.cs|recoverySnapshotRound|Windows local-folder cloud sync test asserts Rust recovery snapshot round"
  "apps/windows/actro.tests/Program.cs|providerDiagnostics.*compatibilityClassification|Windows local-folder cloud membership test asserts Rust provider diagnostics"
  "apps/windows/actro.tests/Program.cs|observabilityEvent.*planType|Windows local-folder cloud membership test asserts Rust observability event"
  "apps/windows/actro.tests/Program.cs|cloudMembershipStatusPolicy|Windows local-folder cloud membership test asserts Rust membership status policy"
  "apps/windows/actro.tests/Program.cs|cloudMembershipScheduleState|Windows local-folder cloud membership test asserts Rust membership schedule state"
  "apps/windows/actro.tests/Program.cs|cloudMembershipAttemptPlan|Windows local-folder cloud membership test asserts Rust membership attempt plan"
  "apps/windows/actro.tests/Program.cs|cloudMembershipLoopPlan|Windows local-folder cloud membership test asserts Rust membership loop plan"
  "apps/windows/actro.tests/Program.cs|finishMembershipAttemptCount|Windows local-folder cloud membership test asserts Rust membership loop summary"
  "apps/windows/actro.tests/Program.cs|cloudProviderAdapterPlan|Windows tests cover Rust provider adapter object-space plan"
  "apps/windows/actro.tests/Program.cs|cloudProviderConnectionStatusPolicy|Windows tests cover Rust provider connection status policy"
  "apps/windows/actro.tests/Program.cs|classifies browser sync cloud provider errors through Rust|Windows tests cover Rust provider error classification"
  "apps/windows/actro.tests/Program.cs|providerErrorClassification|Windows tests cover provider error diagnostics source mapping"
  "apps/windows/actro.tests/Program.cs|blockedJoinResult.*schemaRolloutPlan.*planType|Windows local-folder cloud membership test asserts Rust schema rollout on compatibility block"
  "apps/windows/actro.tests/Program.cs|skips browser storage sync coordinator without credentials|Windows tests cover browser sync credential gating"
  "apps/windows/actro.tests/Program.cs|runs browser storage sync coordinator with credentials|Windows tests cover browser sync coordinator delegation"
  "apps/windows/actro.tests/Program.cs|waitForCredentials|Windows tests cover sync coordinator status policy"
  "apps/windows/actro.tests/Program.cs|schedules browser storage sync retry from status policy|Windows tests cover sync coordinator schedule state"
  "apps/windows/actro.tests/Program.cs|browser storage sync scheduler consumes schedule state and resets after success|Windows tests cover sync scheduler retry lifecycle"
  "apps/windows/actro.tests/Program.cs|retryScheduled|Windows tests cover Rust-owned sync scheduler status"
  "apps/windows/actro.tests/Program.cs|formats browser sync status for product UI|Windows tests cover browser sync status UI policy"
  "apps/windows/actro.tests/Program.cs|persists browser storage sync credentials|Windows tests cover browser sync credential persistence"
  "apps/windows/actro.tests/Program.cs|runs browser storage sync coordinator with stored credentials|Windows tests cover coordinator credential-store integration"
  "apps/windows/actro/src/BrowserHistoryStore.cs|BrowserStorageDatabase|Windows BrowserHistoryStore uses shared browser storage database"
  "apps/windows/actro/src/BrowserBookmarkStore.cs|BrowserStorageDatabase|Windows BrowserBookmarkStore uses shared browser storage database"
  "apps/windows/actro/src/BrowserSessionStore.cs|BrowserStorageDatabase|Windows BrowserSessionStore uses shared browser storage database"
  "crates/actro-core/src/browser_data.rs|browser_history_pages|Rust owns browser history storage schema"
  "crates/actro-core/src/browser_data.rs|select_session|Rust owns browser session read SQL"
  "crates/actro-core/src/browser_data.rs|browser_session_tabs|Rust owns browser session storage schema"
  "crates/actro-core/src/browser_data.rs|tab_id TEXT|Rust browser session schema owns stable tab ids"
  "crates/actro-core/src/browser_data.rs|sync_state TEXT|Rust browser storage schema owns sync state"
  "crates/actro-core/src/browser_data.rs|deleted_at INTEGER|Rust browser storage schema owns tombstone columns"
  "crates/actro-core/src/browser_data.rs|owner_kind TEXT|Rust browser bookmark storage schema owns owner kind"
  "crates/actro-core/src/browser_data.rs|ownerKind|Rust browser bookmark sync payloads include owner kind"
  "crates/actro-core/src/browser_data.rs|surface_kind TEXT NOT NULL DEFAULT 'browser'|Rust browser session storage schema owns surface kind"
  "crates/actro-core/src/browser_data.rs|surfaceKind|Rust browser session sync payloads include surface kind"
  "crates/actro-core/src/browser_data.rs|browser_sync_cursors|Rust browser storage schema owns sync cursors"
  "crates/actro-core/src/browser_data.rs|select_dirty_bookmarks|Rust browser storage schema owns dirty-row sync SQL"
  "crates/actro-core/src/c_api.rs|actro_core_auth_environment_prod|Rust C ABI exposes auth environment defaults"
  "crates/actro-core/src/c_api.rs|actro_core_auth_account_base_url|Rust C ABI exposes auth account base URL"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncBatch|Rust browser storage sync owns typed batch payloads"
  "crates/actro-core/src/browser_data.rs|BROWSER_STORAGE_ADDRESS_CANONICALIZATION_POLICY|Rust browser storage sync owns address canonicalization policy"
  "crates/actro-core/src/browser_data.rs|browser_storage_canonicalize_addresses_json|Rust browser storage sync owns batched address canonicalization"
  "crates/actro-core/src/browser_data.rs|credential_stripped|Rust browser storage sync owns URL credential stripping diagnostics"
  "crates/actro-core/src/browser_data.rs|canonicalization_summary|Rust browser storage sync owns address canonicalization summaries"
  "crates/actro-core/src/browser_data.rs|BROWSER_STORAGE_BOOKMARK_FOLDER_TREE_INTEGRITY_POLICY|Rust browser storage sync owns bookmark folder tree integrity policy"
  "crates/actro-core/src/browser_data.rs|browser_storage_bookmark_folder_tree_integrity_plan_json|Rust browser storage sync owns batched bookmark folder tree integrity planning"
  "crates/actro-core/src/browser_data.rs|integrity_action|Rust browser storage sync owns bookmark folder integrity diagnostics"
  "crates/actro-core/src/browser_data.rs|integrity_summary|Rust browser storage sync owns bookmark folder integrity summaries"
  "crates/actro-core/src/browser_data.rs|BROWSER_STORAGE_BOOKMARK_RESTORE_PLACEMENT_POLICY|Rust browser storage sync owns bookmark restore placement policy"
  "crates/actro-core/src/browser_data.rs|browser_storage_bookmark_restore_placement_plan_json|Rust browser storage sync owns batched bookmark restore placement planning"
  "crates/actro-core/src/browser_data.rs|placement_reason|Rust browser storage sync owns bookmark restore placement reasons"
  "crates/actro-core/src/browser_data.rs|state_conflict_action|Rust browser storage sync owns bookmark delete/restore state diagnostics"
  "crates/actro-core/src/browser_data.rs|state_conflict_reason|Rust browser storage sync owns bookmark delete/restore state reasons"
  "crates/actro-core/src/browser_data.rs|state_conflicts|Rust browser storage sync owns bookmark delete/restore state conflict details"
  "crates/actro-core/src/browser_data.rs|state_conflict_summary|Rust browser storage sync owns bookmark delete/restore state conflict summaries"
  "crates/actro-core/src/browser_data.rs|BrowserStorageBookmarkOwnershipPolicy|Rust browser storage sync owns bookmark ownership policy contract"
  "crates/actro-core/src/browser_data.rs|browser_storage_bookmark_ownership_plan_json|Rust browser storage sync owns batched bookmark ownership planning"
  "crates/actro-core/src/browser_data.rs|retention_reason|Rust browser storage sync owns bookmark ownership retention reason diagnostics"
  "crates/actro-core/src/browser_data.rs|suppressed_bookmark_ids|Rust browser storage sync owns bookmark ownership suppressed source diagnostics"
  "crates/actro-core/src/browser_data.rs|ownership_summary|Rust browser storage sync owns bookmark ownership summaries"
  "crates/actro-core/src/browser_data.rs|BROWSER_STORAGE_HISTORY_VISIT_AGGREGATION_POLICY|Rust browser storage sync owns history visit aggregation policy"
  "crates/actro-core/src/browser_data.rs|browser_storage_history_visit_aggregation_plan_json|Rust browser storage sync owns batched history visit aggregation planning"
  "crates/actro-core/src/browser_data.rs|aggregation_action|Rust browser storage sync owns history visit aggregation diagnostics"
  "crates/actro-core/src/browser_data.rs|aggregation_summary|Rust browser storage sync owns history visit aggregation summaries"
  "crates/actro-core/src/browser_data.rs|BROWSER_STORAGE_SESSION_TAB_ORDERING_POLICY|Rust browser storage sync owns session tab ordering policy"
  "crates/actro-core/src/browser_data.rs|browser_storage_session_tab_ordering_plan_json|Rust browser storage sync owns batched session tab ordering planning"
  "crates/actro-core/src/browser_data.rs|ordering_action|Rust browser storage sync owns session tab ordering diagnostics"
  "crates/actro-core/src/browser_data.rs|ordering_summary|Rust browser storage sync owns session tab ordering summaries"
  "crates/actro-core/src/browser_data.rs|browser_storage_session_tab_id_plan_json|Rust browser storage sync owns batched session tab id normalization"
  "crates/actro-core/src/browser_data.rs|BROWSER_STORAGE_SESSION_ACTIVE_TAB_CONSISTENCY_POLICY|Rust browser storage sync owns session active-tab consistency policy"
  "crates/actro-core/src/browser_data.rs|browser_storage_session_active_tab_consistency_plan_json|Rust browser storage sync owns batched session active-tab consistency planning"
  "crates/actro-core/src/browser_data.rs|normalize_browser_storage_sync_batch_json|Rust browser storage sync normalizes batch payloads"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_batch_from_dirty_rows_json|Rust browser storage sync owns dirty-row batch conversion"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_push_request_from_dirty_rows_json|Rust browser storage sync owns dirty-row push conversion"
  "crates/actro-core/src/browser_data.rs|normalize_browser_storage_sync_ack_batch_json|Rust browser storage sync normalizes ack payloads"
  "crates/actro-core/src/browser_data.rs|normalize_browser_storage_sync_cursors_json|Rust browser storage sync normalizes cursor payloads"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_push_request_json|Rust browser storage sync owns push request payloads"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_pull_request_json|Rust browser storage sync owns pull request payloads"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cycle_response_json|Rust browser storage sync owns cycle response payloads"
  "crates/actro-core/src/browser_data.rs|cursors: Vec<BrowserStorageSyncCursorUpdate>|Rust browser storage sync batches carry cursor handoff payloads"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_client_cycle_plan_json|Rust browser storage sync owns client cycle apply/ack planning"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_client_ack_batch_after_remote_apply_json|Rust browser storage sync owns post-remote safe ack selection"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_client_cycle_apply_result_json|Rust browser storage sync owns sync cycle apply result adjudication"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_transport_contract_json|Rust browser storage sync owns transport endpoint contract"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudStore|Rust browser storage sync owns cloud provider object-store trait"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_store_contract_json|Rust browser storage sync owns cloud provider object-store contract"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudProviderAdapterPlan|Rust browser storage sync owns cloud provider adapter object-space plan"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_provider_adapter_contract_json|Rust browser storage sync exports cloud provider adapter contract"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_provider_adapter_plan_json|Rust browser storage sync exports cloud provider adapter plan"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudProviderErrorClassificationPlan|Rust browser storage sync owns cloud provider error classification"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_provider_error_policy_json|Rust browser storage sync exports cloud provider error policy"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_provider_error_classification_json|Rust browser storage sync exports cloud provider error classifier"
  "crates/actro-core/src/browser_data.rs|operation_boundary|Rust browser storage sync keeps provider adapter IO coarse-grained"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudDataModelContract|Rust browser storage sync owns cloud document data model"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_data_model_json|Rust browser storage sync exports cloud document data model"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_default_manifest_json|Rust browser storage sync owns cloud manifest defaults"
  "crates/actro-core/src/browser_data.rs|normalize_browser_storage_sync_cloud_manifest_json|Rust browser storage sync owns cloud manifest normalization"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_log_object_json|Rust browser storage sync owns cloud log object building"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_snapshot_object_json|Rust browser storage sync owns cloud snapshot object building"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudPushPlan|Rust browser storage sync owns cloud push write plans"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudPushWriteSummary|Rust browser storage sync owns cloud push write summaries"
  "crates/actro-core/src/browser_data.rs|write_summary|Rust browser storage sync embeds cloud push write summaries in push plans"
  "crates/actro-core/src/browser_data.rs|cloud_push_plan_builds_ordered_provider_writes|Rust browser storage sync tests cloud push write summaries"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_push_plan_json|Rust browser storage sync exports cloud push write plans"
  "crates/actro-core/src/browser_data.rs|write_reason|Rust browser storage sync records cloud planned write reasons"
  "crates/actro-core/src/browser_data.rs|write_reason: operation.write_reason.clone|Rust browser storage sync propagates planned write reasons into result issues"
  "crates/actro-core/src/browser_data.rs|write_reason: Some|Rust browser storage sync propagates planned write reasons into compaction issues"
  "crates/actro-core/src/browser_data.rs|delete_reason: Some|Rust browser storage sync propagates planned delete reasons into compaction issues"
  "crates/actro-core/src/browser_data.rs|delete_reason: operation.delete_reason.clone|Rust browser storage sync propagates planned delete reasons into cleanup issues"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudPushResultPlan|Rust browser storage sync owns cloud push result plans"
  "crates/actro-core/src/browser_data.rs|ack_reason|Rust browser storage sync owns cloud push ack reasons"
  "crates/actro-core/src/browser_data.rs|allRequiredCloudWritesSucceeded|Rust browser storage sync records committed cloud-write ack reason"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_push_result_plan_json|Rust browser storage sync exports cloud push result plans"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudPullPlan|Rust browser storage sync owns cloud pull read plans"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudPullReadSummary|Rust browser storage sync owns cloud pull read summaries"
  "crates/actro-core/src/browser_data.rs|read_summary|Rust browser storage sync embeds cloud pull read summaries in pull plans"
  "crates/actro-core/src/browser_data.rs|cloud_pull_plan_lists_logs_and_snapshots_from_manifest_state|Rust browser storage sync tests cloud pull read summaries"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_pull_plan_json|Rust browser storage sync exports cloud pull read plans"
  "crates/actro-core/src/browser_data.rs|read_reason|Rust browser storage sync owns cloud planned read reasons"
  "crates/actro-core/src/browser_data.rs|readListedCloudLogObject|Rust browser storage sync records listed log-object read reasons"
  "crates/actro-core/src/browser_data.rs|read_reason: operation.read_reason.clone|Rust browser storage sync propagates planned read reasons into list results"
  "crates/actro-core/src/browser_data.rs|read_reason: browser_storage_sync_cloud_planned_read_reason|Rust browser storage sync records direct read result reasons"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudPullApplyPlan|Rust browser storage sync owns cloud pull apply plans"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudPullApplySummary|Rust browser storage sync owns cloud pull apply summaries"
  "crates/actro-core/src/browser_data.rs|apply_summary|Rust browser storage sync embeds cloud pull apply summaries in pull apply plans"
  "crates/actro-core/src/browser_data.rs|cloud_pull_apply_plan_builds_remote_batch_from_provider_reads|Rust browser storage sync tests cloud pull apply summaries"
  "crates/actro-core/src/browser_data.rs|apply_action|Rust browser storage sync owns cloud pull apply action diagnostics"
  "crates/actro-core/src/browser_data.rs|remoteBatchContainsRecords|Rust browser storage sync records remote-apply record reason"
  "crates/actro-core/src/browser_data.rs|onlySkippedCloudInputs|Rust browser storage sync records no-change skipped-input reason"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudPullApplySkippedSnapshot|Rust browser storage sync owns cloud pull apply skipped-snapshot diagnostics"
  "crates/actro-core/src/browser_data.rs|skipSnapshotForIncrementalPull|Rust browser storage sync records incremental skipped-snapshot action"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudPullApplySkippedLog|Rust browser storage sync owns cloud pull apply skipped-log diagnostics"
  "crates/actro-core/src/browser_data.rs|skipSelfOriginatedLog|Rust browser storage sync records same-device skipped-log action"
  "crates/actro-core/src/browser_data.rs|skipRedundantLog|Rust browser storage sync records redundant skipped-log action"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_pull_apply_plan_json|Rust browser storage sync exports cloud pull apply plans"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudPullContinuationPlan|Rust browser storage sync owns cloud pull pagination continuation plans"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudPullContinuationSummary|Rust browser storage sync owns cloud pull continuation summaries"
  "crates/actro-core/src/browser_data.rs|continuation_summary|Rust browser storage sync embeds cloud pull continuation summaries in continuation plans"
  "crates/actro-core/src/browser_data.rs|cloud_pull_continuation_plan_preserves_provider_page_cursors|Rust browser storage sync tests cloud pull continuation summaries"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_pull_continuation_plan_json|Rust browser storage sync exports cloud pull pagination continuation plans"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudCompactionPlan|Rust browser storage sync owns cloud compaction plans"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_compaction_plan_json|Rust browser storage sync exports cloud compaction plans"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudCompactionResultPlan|Rust browser storage sync owns cloud compaction result plans"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_compaction_result_plan_json|Rust browser storage sync exports cloud compaction result plans"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudCompactionRoundResult|Rust browser storage sync owns cloud compaction round results"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudCompactionRoundSummary|Rust browser storage sync owns cloud compaction round summaries"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_compaction_round_summary|Rust browser storage sync builds cloud compaction round summaries"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_run_compaction_round|Rust browser storage sync exports cloud compaction round runner"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_local_folder_compaction_round_json|Rust browser storage sync exports local-folder compaction round JSON"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudHighWatermarkPolicy|Rust browser storage sync owns cloud high-watermark policy"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudHighWatermarkPlan|Rust browser storage sync owns cloud high-watermark retention plans"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_high_watermark_plan_json|Rust browser storage sync exports cloud high-watermark retention plans"
  "crates/actro-core/src/browser_data.rs|delete_reason|Rust browser storage sync owns cloud planned delete reasons"
  "crates/actro-core/src/browser_data.rs|cloud_high_watermark_plan_marks_lagging_and_cleanup_candidates|Rust browser storage sync tests cloud high-watermark cleanup candidates"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudCleanupResultPlan|Rust browser storage sync owns cloud cleanup result plans"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_cleanup_result_plan_json|Rust browser storage sync exports cloud cleanup result plans"
  "crates/actro-core/src/browser_data.rs|cloud_cleanup_result_plan_retries_transient_delete_failures|Rust browser storage sync tests cloud cleanup result retry planning"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudCleanupRoundResult|Rust browser storage sync owns cloud cleanup round results"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudCleanupRoundSummary|Rust browser storage sync owns cloud cleanup round summaries"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_cleanup_round_summary|Rust browser storage sync builds cloud cleanup round summaries"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_run_cleanup_round|Rust browser storage sync owns cloud cleanup round execution"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_local_folder_cleanup_round_json|Rust browser storage sync exports local-folder cleanup round JSON"
  "crates/actro-core/src/browser_data.rs|cloud_local_folder_cleanup_round_json_deletes_retained_old_logs|Rust browser storage sync tests local-folder cleanup rounds against real files"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_recovery_retention_plan|Rust browser storage sync owns cloud recovery-snapshot retention planning"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudRecoveryRetentionResultPlan|Rust browser storage sync owns cloud recovery retention result plans"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_run_recovery_retention_round|Rust browser storage sync owns cloud recovery retention round execution"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudRecoveryRetentionRoundSummary|Rust browser storage sync owns cloud recovery retention round summaries"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_local_folder_recovery_retention_round_json|Rust browser storage sync exports local-folder recovery retention round JSON"
  "crates/actro-core/src/browser_data.rs|cloud_local_folder_recovery_retention_round_json_deletes_oldest_roots|Rust browser storage sync tests local-folder recovery retention rounds against real files"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_local_folder_recovery_retention_round_json|Rust browser storage sync exposes local-folder recovery retention round C ABI"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncMemoryCloudStore|Rust browser storage sync owns in-memory cloud store fixture"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncLocalFolderCloudStore|Rust browser storage sync owns local-folder cloud store fixture"
  "crates/actro-core/src/browser_data.rs|cloud_local_folder_store_runs_push_round_against_real_files|Rust browser storage sync tests local-folder cloud push rounds against real files"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_execute_planned_writes|Rust browser storage sync owns cloud planned write execution"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_execute_planned_deletes|Rust browser storage sync owns cloud planned delete execution"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudReadOperationResult|Rust browser storage sync owns cloud read result classification"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudPushRoundResult|Rust browser storage sync owns cloud push round result classification"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudPushRoundSummary|Rust browser storage sync owns cloud push round summaries"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_push_round_summary|Rust browser storage sync builds cloud push round summaries"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_run_push_round|Rust browser storage sync owns cloud push round execution"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudExecutedReadOperations|Rust browser storage sync owns cloud planned read execution results"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudPullRoundResult|Rust browser storage sync owns cloud pull round result classification"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudPullRoundSummary|Rust browser storage sync owns cloud pull round summaries"
  "crates/actro-core/src/browser_data.rs|round_summary|Rust browser storage sync embeds cloud pull round summaries in pull rounds"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_execute_planned_reads|Rust browser storage sync owns cloud planned read execution"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_execute_planned_reads_with_continuations|Rust browser storage sync owns paginated cloud planned read execution"
  "crates/actro-core/src/browser_data.rs|BROWSER_STORAGE_SYNC_CLOUD_MAX_PULL_CONTINUATION_PAGES|Rust browser storage sync bounds pull continuation execution"
  "crates/actro-core/src/browser_data.rs|paginationLimitExceeded|Rust browser storage sync diagnoses pull pagination safety limit"
  "crates/actro-core/src/browser_data.rs|cloud_pull_round_follows_log_pagination_continuations|Rust browser storage sync tests paginated cloud pull execution"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_run_pull_round|Rust browser storage sync owns cloud pull round execution"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudCycleRoundResult|Rust browser storage sync owns cloud cycle round result classification"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudCycleRoundSummary|Rust browser storage sync owns cloud cycle round summaries"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_cycle_round_summary|Rust browser storage sync builds cloud cycle round summaries"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_run_cycle_round|Rust browser storage sync owns cloud cycle round execution"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudCycleClientPlan|Rust browser storage sync owns cloud cycle client planning"
  "crates/actro-core/src/browser_data.rs|step_reason|Rust browser storage sync owns cloud cycle client step reasons"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_cycle_client_plan_json|Rust browser storage sync owns cloud cycle client plan JSON helper"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudCycleStatusPolicy|Rust browser storage sync owns cloud cycle status policy"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_cycle_status_policy_json|Rust browser storage sync owns cloud cycle status policy JSON helper"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudMembershipStatusPolicy|Rust browser storage sync owns cloud membership status policy"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_membership_status_policy_json|Rust browser storage sync owns cloud membership status policy JSON helper"
  "crates/actro-core/src/browser_data.rs|cloud_membership_status_policy_classifies_membership_rounds|Rust browser storage sync tests cloud membership status policy"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudCycleScheduleState|Rust browser storage sync owns cloud cycle schedule state"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_cycle_schedule_state_json|Rust browser storage sync owns cloud cycle schedule state JSON helper"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudMembershipScheduleState|Rust browser storage sync owns cloud membership schedule state"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_membership_schedule_state_json|Rust browser storage sync owns cloud membership schedule state JSON helper"
  "crates/actro-core/src/browser_data.rs|cloud_membership_schedule_state_derives_provider_native_timing_and_flags|Rust browser storage sync tests cloud membership schedule state"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudMembershipAttemptPlan|Rust browser storage sync owns cloud membership attempt planning"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_membership_attempt_plan_json|Rust browser storage sync owns cloud membership attempt plan JSON helper"
  "crates/actro-core/src/browser_data.rs|cloud_membership_attempt_plan_composes_retry_and_recovery_paths|Rust browser storage sync tests cloud membership attempt plan"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudMembershipLoopPlan|Rust browser storage sync owns cloud membership loop planning"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudMembershipLoopSummary|Rust browser storage sync owns cloud membership loop summaries"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_membership_loop_summary|Rust browser storage sync builds cloud membership loop summaries"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_membership_loop_plan_json|Rust browser storage sync owns cloud membership loop plan JSON helper"
  "crates/actro-core/src/browser_data.rs|cloud_membership_loop_plan_bounds_connect_revoke_rounds|Rust browser storage sync tests cloud membership loop plan"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudProviderDiagnostics|Rust browser storage sync owns cloud provider diagnostics"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_provider_diagnostics_json|Rust browser storage sync owns cloud provider diagnostics JSON helper"
  "crates/actro-core/src/browser_data.rs|provider_error_classification_json|Rust cloud provider diagnostics accepts provider error classification"
  "crates/actro-core/src/browser_data.rs|provider_error_classification|Rust cloud provider diagnostics exposes provider error classification source"
  "crates/actro-core/src/browser_data.rs|cloud_provider_error_classification_prioritizes_recovery_and_retry|Rust browser storage sync tests cloud provider error classification priority"
  "crates/actro-core/src/browser_data.rs|cloud_provider_diagnostics_accepts_provider_error_classification_source|Rust browser storage sync tests provider error diagnostics source mapping"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_provider_error_collect_items|Rust cloud provider error classifier extracts nested round/result provider failures"
  "crates/actro-core/src/browser_data.rs|cloud_provider_error_classification_extracts_nested_round_failures|Rust browser storage sync tests nested provider round failure extraction"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudObservabilityEvent|Rust browser storage sync owns cloud observability events"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_observability_event_json|Rust browser storage sync owns cloud observability event JSON helper"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudCycleAttemptPlan|Rust browser storage sync owns cloud cycle attempt planning"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_cycle_attempt_plan_json|Rust browser storage sync owns cloud cycle attempt plan JSON helper"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudCycleLoopPlan|Rust browser storage sync owns cloud cycle loop planning"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudCycleLoopSummary|Rust browser storage sync owns cloud cycle loop summaries"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_cycle_loop_summary|Rust browser storage sync builds cloud cycle loop summaries"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_cycle_loop_plan_json|Rust browser storage sync owns cloud cycle loop plan JSON helper"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_local_folder_capabilities_json|Rust browser storage sync exports local-folder capabilities JSON helper"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_local_folder_capability_preflight_json|Rust browser storage sync exports local-folder capability preflight JSON helper"
  "crates/actro-core/src/browser_data.rs|cloud_local_folder_capability_preflight_uses_rust_owned_capabilities|Rust browser storage sync tests Rust-owned local-folder capability preflight"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_run_compatibility_preflight|Rust browser storage sync owns generic cloud compatibility preflight execution"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_local_folder_compatibility_preflight_json|Rust browser storage sync exports local-folder compatibility preflight JSON helper"
  "crates/actro-core/src/browser_data.rs|cloud_local_folder_compatibility_preflight_reads_real_files|Rust browser storage sync tests local-folder compatibility preflight against real files"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_local_folder_cycle_round_json|Rust browser storage sync exposes local-folder cloud cycle JSON helper"
  "crates/actro-core/src/browser_data.rs|cloud_local_folder_cycle_round_json_runs_against_real_files|Rust browser storage sync tests local-folder cycle JSON against real files"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_local_folder_membership_round_json|Rust browser storage sync exposes local-folder cloud membership JSON helper"
  "crates/actro-core/src/browser_data.rs|cloud_local_folder_membership_round_json_runs_against_real_files|Rust browser storage sync tests local-folder membership JSON against real files"
  "crates/actro-core/src/browser_data.rs|compatibility_preflight: Option<BrowserStorageSyncCloudCompatibilityPreflightPlan>|Rust browser storage sync returns membership compatibility preflight"
  "crates/actro-core/src/browser_data.rs|cloudCompatibilityPreflightUnavailable|Rust browser storage sync blocks membership when compatibility preflight cannot run"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudStoreCapabilityPreflightPlan|Rust browser storage sync owns cloud store capability preflight"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_store_capability_preflight_json|Rust browser storage sync exports cloud store capability preflight"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudCompatibilityStatus|Rust browser storage sync owns cloud compatibility status"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_compatibility_status_json|Rust browser storage sync exports cloud compatibility status"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudCompatibilityPreflightPlan|Rust browser storage sync owns batched cloud compatibility preflight"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_compatibility_preflight_json|Rust browser storage sync exports batched cloud compatibility preflight"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudSchemaRolloutPlan|Rust browser storage sync owns cloud schema rollout planning"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_schema_rollout_plan_json|Rust browser storage sync exports cloud schema rollout planning"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudSchemaMigrationPlaybook|Rust browser storage sync owns cloud schema migration playbook"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_schema_migration_playbook_json|Rust browser storage sync exports cloud schema migration playbook"
  "crates/actro-core/src/browser_data.rs|browser_storage_schema_migration_plan_json|Rust browser storage owns the incremental schema migration plan DDL"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudRecoverySnapshotPlan|Rust browser storage sync owns cloud recovery snapshot planning"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_recovery_snapshot_plan_json|Rust browser storage sync exports cloud recovery snapshot planning"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudRecoverySnapshotResultPlan|Rust browser storage sync owns cloud recovery snapshot result handoff"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_recovery_snapshot_result_plan_json|Rust browser storage sync exports cloud recovery snapshot result handoff"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudRecoverySnapshotRoundResult|Rust browser storage sync owns cloud recovery snapshot round results"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudRecoverySnapshotRoundSummary|Rust browser storage sync owns cloud recovery snapshot round summaries"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_recovery_snapshot_round_summary|Rust browser storage sync builds cloud recovery snapshot round summaries"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_run_recovery_snapshot_round|Rust browser storage sync owns the cloud recovery snapshot round runner"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_local_folder_recovery_snapshot_round_json|Rust browser storage sync exports local-folder recovery snapshot rounds"
  "crates/actro-core/src/browser_data.rs|recoverySnapshotRound|Rust browser storage sync emits recovery snapshot round plans"
  "crates/actro-core/src/browser_data.rs|cloudRecoverySnapshotResult|Rust browser storage sync emits cloud recovery snapshot result plans"
  "crates/actro-core/src/browser_data.rs|readRecoverySnapshotSource|Rust browser storage sync emits recovery snapshot source rereads"
  "crates/actro-core/src/browser_data.rs|writeRecoveryCopy|Rust browser storage sync emits cloud recovery snapshot copy operations"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudMembershipPlan|Rust browser storage sync owns cloud device membership planning"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_membership_plan_json|Rust browser storage sync exports cloud device membership planning"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudMembershipResultPlan|Rust browser storage sync owns cloud device membership result planning"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_membership_result_plan_json|Rust browser storage sync exports cloud device membership result planning"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudMembershipRoundSummary|Rust browser storage sync owns cloud membership round summaries"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_membership_round_summary|Rust browser storage sync builds cloud membership round summaries"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cloud_run_membership_round|Rust browser storage sync owns cloud membership round execution"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_pull_continuation_plan_json|Rust UniFFI contract exposes browser sync cloud pull continuation plans"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_membership_status_policy_json|Rust UniFFI contract exposes browser sync cloud membership status policy"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_cycle_schedule_state_json|Rust UniFFI contract exposes browser sync cloud cycle schedule state"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_membership_schedule_state_json|Rust UniFFI contract exposes browser sync cloud membership schedule state"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_membership_attempt_plan_json|Rust UniFFI contract exposes browser sync cloud membership attempt plans"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_membership_loop_plan_json|Rust UniFFI contract exposes browser sync cloud membership loop plans"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_provider_diagnostics_json|Rust UniFFI contract exposes browser sync cloud provider diagnostics"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_observability_event_json|Rust UniFFI contract exposes browser sync cloud observability events"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_cycle_attempt_plan_json|Rust UniFFI contract exposes browser sync cloud cycle attempt plan"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_cycle_loop_plan_json|Rust UniFFI contract exposes browser sync cloud cycle loop plan"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_local_folder_capabilities_json|Rust UniFFI contract exposes browser sync local-folder capabilities"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_local_folder_capability_preflight_json|Rust UniFFI contract exposes browser sync local-folder capability preflight"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_local_folder_compatibility_preflight_json|Rust UniFFI contract exposes browser sync local-folder compatibility preflight"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_local_folder_cycle_round_json|Rust UniFFI contract exposes browser sync local-folder cycle rounds"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_local_folder_membership_round_json|Rust UniFFI contract exposes browser sync local-folder membership rounds"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_store_capability_preflight_json|Rust UniFFI contract exposes browser sync cloud store capability preflight"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_compatibility_status_json|Rust UniFFI contract exposes browser sync cloud compatibility status"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_compatibility_preflight_json|Rust UniFFI contract exposes browser sync cloud compatibility preflight"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_schema_rollout_plan_json|Rust UniFFI contract exposes browser sync cloud schema rollout planning"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_schema_migration_playbook_json|Rust UniFFI contract exposes browser sync cloud schema migration playbook"
  "crates/actro-core/src/actro_core.udl|browser_storage_schema_migration_plan_json|Rust UniFFI contract exposes the browser storage schema migration plan"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_recovery_snapshot_plan_json|Rust UniFFI contract exposes browser sync cloud recovery snapshot planning"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_recovery_snapshot_result_plan_json|Rust UniFFI contract exposes browser sync cloud recovery snapshot result handoff"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_local_folder_recovery_snapshot_round_json|Rust UniFFI contract exposes browser sync local-folder recovery snapshot rounds"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_membership_plan_json|Rust UniFFI contract exposes browser sync cloud membership plans"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_membership_result_plan_json|Rust UniFFI contract exposes browser sync cloud membership result plans"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_high_watermark_policy_json|Rust UniFFI contract exposes browser sync cloud high-watermark policy"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_high_watermark_plan_json|Rust UniFFI contract exposes browser sync cloud high-watermark plans"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_cleanup_result_plan_json|Rust UniFFI contract exposes browser sync cloud cleanup result plans"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_local_folder_cleanup_round_json|Rust UniFFI contract exposes browser sync local-folder cleanup round"
  "crates/actro-core/src/actro_core.udl|browser_storage_address_canonicalization_policy_json|Rust UniFFI contract exposes address canonicalization policy"
  "crates/actro-core/src/actro_core.udl|browser_storage_canonicalize_addresses_json|Rust UniFFI contract exposes batched address canonicalization"
  "crates/actro-core/src/actro_core.udl|browser_storage_bookmark_folder_tree_integrity_policy_json|Rust UniFFI contract exposes bookmark folder tree integrity policy"
  "crates/actro-core/src/actro_core.udl|browser_storage_bookmark_folder_tree_integrity_plan_json|Rust UniFFI contract exposes bookmark folder tree integrity planning"
  "crates/actro-core/src/actro_core.udl|browser_storage_bookmark_restore_placement_policy_json|Rust UniFFI contract exposes bookmark restore placement policy"
  "crates/actro-core/src/actro_core.udl|browser_storage_bookmark_restore_placement_plan_json|Rust UniFFI contract exposes bookmark restore placement planning"
  "crates/actro-core/src/actro_core.udl|browser_storage_bookmark_ownership_policy_json|Rust UniFFI contract exposes bookmark ownership policy"
  "crates/actro-core/src/actro_core.udl|browser_storage_bookmark_ownership_plan_json|Rust UniFFI contract exposes bookmark ownership planning"
  "crates/actro-core/src/actro_core.udl|browser_storage_history_visit_aggregation_policy_json|Rust UniFFI contract exposes history visit aggregation policy"
  "crates/actro-core/src/actro_core.udl|browser_storage_history_visit_aggregation_plan_json|Rust UniFFI contract exposes history visit aggregation planning"
  "crates/actro-core/src/actro_core.udl|browser_storage_session_tab_ordering_policy_json|Rust UniFFI contract exposes session tab ordering policy"
  "crates/actro-core/src/actro_core.udl|browser_storage_session_tab_ordering_plan_json|Rust UniFFI contract exposes session tab ordering planning"
  "crates/actro-core/src/actro_core.udl|browser_storage_session_tab_id_plan_json|Rust UniFFI contract exposes session tab id planning"
  "crates/actro-core/src/actro_core.udl|browser_storage_session_active_tab_consistency_policy_json|Rust UniFFI contract exposes session active-tab consistency policy"
  "crates/actro-core/src/actro_core.udl|browser_storage_session_active_tab_consistency_plan_json|Rust UniFFI contract exposes session active-tab consistency planning"
  "crates/actro-core/src/actro_core.udl|browser_storage_retention_pruning_policy_json|Rust UniFFI contract exposes retention pruning policy"
  "crates/actro-core/src/actro_core.udl|browser_storage_retention_pruning_plan_json|Rust UniFFI contract exposes retention pruning planning"
  "crates/actro-core/src/browser_data.rs|BROWSER_STORAGE_SYNC_CLOUD_MANIFEST_PATH|Rust browser storage sync owns cloud manifest path constants"
  "crates/actro-core/src/browser_data.rs|expected_revision|Rust browser storage sync cloud store uses optimistic provider revisions"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_provider_adapter_contract_json|Rust UniFFI contract exposes provider adapter contract"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_provider_adapter_plan_json|Rust UniFFI contract exposes provider adapter planning"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_provider_connection_status_policy_json|Rust UniFFI contract exposes provider connection status policy"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_provider_error_policy_json|Rust UniFFI contract exposes provider error policy"
  "crates/actro-core/src/actro_core.udl|browser_storage_sync_cloud_provider_error_classification_json|Rust UniFFI contract exposes provider error classification"
  "crates/actro-core/src/actro_core.udl|provider_error_classification_json|Rust UniFFI diagnostics contract accepts provider error classification"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_provider_adapter_contract_json|Rust C ABI exposes provider adapter contract"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_provider_adapter_plan_json|Rust C ABI exposes provider adapter plan"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_provider_adapter_contract_json|Rust C header exposes provider adapter contract"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_provider_adapter_plan_json|Rust C header exposes provider adapter plan"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncCloudProviderConnectionStatusPolicy|Rust browser storage sync owns provider connection status policy"
  "crates/actro-core/src/browser_data.rs|should_attempt_membership|Rust provider connection policy can hand off cloud membership"
  "crates/actro-core/src/browser_data.rs|No Actro product server allocates provider account|Rust provider connection contract rejects service-side account allocation"
  "crates/actro-core/src/ffi.rs|browser_storage_sync_cloud_provider_error_classification_json|Rust UniFFI wrapper exposes provider error classification"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_provider_connection_contract_json|Rust C ABI exposes provider connection contract"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_provider_connection_status_policy_json|Rust C ABI exposes provider connection status policy"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_provider_error_policy_json|Rust C ABI exposes provider error policy"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_provider_error_classification_json|Rust C ABI exposes provider error classification"
  "crates/actro-core/src/c_api.rs|provider_error_classification_json|Rust C ABI diagnostics accepts provider error classification"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_store_capability_preflight_json|Rust C ABI exposes browser sync cloud store capability preflight"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_data_model_json|Rust C ABI exposes browser sync cloud data model"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_compatibility_status_json|Rust C ABI exposes browser sync cloud compatibility status"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_compatibility_preflight_json|Rust C ABI exposes browser sync cloud compatibility preflight"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_schema_rollout_plan_json|Rust C ABI exposes browser sync cloud schema rollout planning"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_schema_migration_playbook_json|Rust C ABI exposes browser sync cloud schema migration playbook"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_schema_migration_plan_json|Rust C ABI exposes the browser storage schema migration plan"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_recovery_snapshot_plan_json|Rust C ABI exposes browser sync cloud recovery snapshot planning"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_recovery_snapshot_result_plan_json|Rust C ABI exposes browser sync cloud recovery snapshot result handoff"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_local_folder_recovery_snapshot_round_json|Rust C ABI exposes browser sync local-folder recovery snapshot rounds"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_membership_plan_json|Rust C ABI exposes browser sync cloud membership plans"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_membership_result_plan_json|Rust C ABI exposes browser sync cloud membership result plans"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_push_plan_json|Rust C ABI exposes browser sync cloud push plans"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_push_result_plan_json|Rust C ABI exposes browser sync cloud push result plans"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_pull_plan_json|Rust C ABI exposes browser sync cloud pull plans"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_pull_apply_plan_json|Rust C ABI exposes browser sync cloud pull apply plans"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_pull_continuation_plan_json|Rust C ABI exposes browser sync cloud pull continuation plans"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_compaction_plan_json|Rust C ABI exposes browser sync cloud compaction plans"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_compaction_result_plan_json|Rust C ABI exposes browser sync cloud compaction result plans"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_local_folder_compaction_round_json|Rust C ABI exposes browser sync local-folder compaction round"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_high_watermark_policy_json|Rust C ABI exposes browser sync cloud high-watermark policy"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_high_watermark_plan_json|Rust C ABI exposes browser sync cloud high-watermark plans"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_cleanup_result_plan_json|Rust C ABI exposes browser sync cloud cleanup result plans"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_local_folder_cleanup_round_json|Rust C ABI exposes browser sync local-folder cleanup round"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_cycle_client_plan_json|Rust C ABI exposes browser sync cloud cycle client plans"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_cycle_status_policy_json|Rust C ABI exposes browser sync cloud cycle status policy"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_membership_status_policy_json|Rust C ABI exposes browser sync cloud membership status policy"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_cycle_schedule_state_json|Rust C ABI exposes browser sync cloud cycle schedule state"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_membership_schedule_state_json|Rust C ABI exposes browser sync cloud membership schedule state"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_membership_attempt_plan_json|Rust C ABI exposes browser sync cloud membership attempt plans"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_membership_loop_plan_json|Rust C ABI exposes browser sync cloud membership loop plans"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_provider_diagnostics_json|Rust C ABI exposes browser sync cloud provider diagnostics"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_observability_event_json|Rust C ABI exposes browser sync cloud observability events"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_cycle_attempt_plan_json|Rust C ABI exposes browser sync cloud cycle attempt plan"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_cycle_loop_plan_json|Rust C ABI exposes browser sync cloud cycle loop plan"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_local_folder_capabilities_json|Rust C ABI exposes browser sync local-folder capabilities"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_local_folder_capability_preflight_json|Rust C ABI exposes browser sync local-folder capability preflight"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_local_folder_compatibility_preflight_json|Rust C ABI exposes browser sync local-folder compatibility preflight"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_local_folder_cycle_round_json|Rust C ABI exposes browser sync local-folder cycle rounds"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_sync_cloud_local_folder_membership_round_json|Rust C ABI exposes browser sync local-folder membership rounds"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_address_canonicalization_policy_json|Rust C ABI exposes address canonicalization policy"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_canonicalize_addresses_json|Rust C ABI exposes batched address canonicalization"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_bookmark_folder_tree_integrity_policy_json|Rust C ABI exposes bookmark folder tree integrity policy"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_bookmark_folder_tree_integrity_plan_json|Rust C ABI exposes bookmark folder tree integrity planning"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_bookmark_restore_placement_policy_json|Rust C ABI exposes bookmark restore placement policy"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_bookmark_restore_placement_plan_json|Rust C ABI exposes bookmark restore placement planning"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_bookmark_ownership_policy_json|Rust C ABI exposes bookmark ownership policy"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_bookmark_ownership_plan_json|Rust C ABI exposes bookmark ownership planning"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_history_visit_aggregation_policy_json|Rust C ABI exposes history visit aggregation policy"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_history_visit_aggregation_plan_json|Rust C ABI exposes history visit aggregation planning"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_session_tab_ordering_policy_json|Rust C ABI exposes session tab ordering policy"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_session_tab_ordering_plan_json|Rust C ABI exposes session tab ordering planning"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_session_tab_id_plan_json|Rust C ABI exposes session tab id planning"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_session_active_tab_consistency_policy_json|Rust C ABI exposes session active-tab consistency policy"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_session_active_tab_consistency_plan_json|Rust C ABI exposes session active-tab consistency planning"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_retention_pruning_policy_json|Rust C ABI exposes retention pruning policy"
  "crates/actro-core/src/c_api.rs|actro_core_browser_storage_retention_pruning_plan_json|Rust C ABI exposes retention pruning planning"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_compatibility_status_json|Rust C header exposes browser sync cloud compatibility status"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_provider_connection_contract_json|Rust C header exposes provider connection contract"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_provider_connection_status_policy_json|Rust C header exposes provider connection status policy"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_provider_error_policy_json|Rust C header exposes provider error policy"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_provider_error_classification_json|Rust C header exposes provider error classification"
  "crates/actro-core/include/actro_core.h|provider_error_classification_json|Rust C header diagnostics accepts provider error classification"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_store_capability_preflight_json|Rust C header exposes browser sync cloud store capability preflight"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_compatibility_preflight_json|Rust C header exposes browser sync cloud compatibility preflight"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_schema_rollout_plan_json|Rust C header exposes browser sync cloud schema rollout planning"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_schema_migration_playbook_json|Rust C header exposes browser sync cloud schema migration playbook"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_schema_migration_plan_json|Rust C header exposes the browser storage schema migration plan"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_recovery_snapshot_plan_json|Rust C header exposes browser sync cloud recovery snapshot planning"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_recovery_snapshot_result_plan_json|Rust C header exposes browser sync cloud recovery snapshot result handoff"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_local_folder_recovery_snapshot_round_json|Rust C header exposes browser sync local-folder recovery snapshot rounds"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_membership_plan_json|Rust C header exposes browser sync cloud membership plans"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_membership_result_plan_json|Rust C header exposes browser sync cloud membership result plans"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_pull_continuation_plan_json|Rust C header exposes browser sync cloud pull continuation plans"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_high_watermark_policy_json|Rust C header exposes browser sync cloud high-watermark policy"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_high_watermark_plan_json|Rust C header exposes browser sync cloud high-watermark plans"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_cleanup_result_plan_json|Rust C header exposes browser sync cloud cleanup result plans"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_local_folder_cleanup_round_json|Rust C header exposes browser sync local-folder cleanup round"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_cycle_client_plan_json|Rust C header exposes browser sync cloud cycle client plans"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_cycle_status_policy_json|Rust C header exposes browser sync cloud cycle status policy"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_membership_status_policy_json|Rust C header exposes browser sync cloud membership status policy"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_cycle_schedule_state_json|Rust C header exposes browser sync cloud cycle schedule state"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_membership_schedule_state_json|Rust C header exposes browser sync cloud membership schedule state"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_membership_attempt_plan_json|Rust C header exposes browser sync cloud membership attempt plans"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_membership_loop_plan_json|Rust C header exposes browser sync cloud membership loop plans"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_provider_diagnostics_json|Rust C header exposes browser sync cloud provider diagnostics"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_observability_event_json|Rust C header exposes browser sync cloud observability events"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_cycle_attempt_plan_json|Rust C header exposes browser sync cloud cycle attempt plan"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_cycle_loop_plan_json|Rust C header exposes browser sync cloud cycle loop plan"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_local_folder_capabilities_json|Rust C header exposes browser sync local-folder capabilities"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_local_folder_capability_preflight_json|Rust C header exposes browser sync local-folder capability preflight"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_local_folder_compatibility_preflight_json|Rust C header exposes browser sync local-folder compatibility preflight"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_local_folder_cycle_round_json|Rust C header exposes browser sync local-folder cycle rounds"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_sync_cloud_local_folder_membership_round_json|Rust C header exposes browser sync local-folder membership rounds"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_address_canonicalization_policy_json|Rust C header exposes address canonicalization policy"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_canonicalize_addresses_json|Rust C header exposes batched address canonicalization"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_bookmark_folder_tree_integrity_policy_json|Rust C header exposes bookmark folder tree integrity policy"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_bookmark_folder_tree_integrity_plan_json|Rust C header exposes bookmark folder tree integrity planning"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_bookmark_restore_placement_policy_json|Rust C header exposes bookmark restore placement policy"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_bookmark_restore_placement_plan_json|Rust C header exposes bookmark restore placement planning"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_bookmark_ownership_policy_json|Rust C header exposes bookmark ownership policy"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_bookmark_ownership_plan_json|Rust C header exposes bookmark ownership planning"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_history_visit_aggregation_policy_json|Rust C header exposes history visit aggregation policy"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_history_visit_aggregation_plan_json|Rust C header exposes history visit aggregation planning"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_session_tab_ordering_policy_json|Rust C header exposes session tab ordering policy"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_session_tab_ordering_plan_json|Rust C header exposes session tab ordering planning"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_session_tab_id_plan_json|Rust C header exposes session tab id planning"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_session_active_tab_consistency_policy_json|Rust C header exposes session active-tab consistency policy"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_session_active_tab_consistency_plan_json|Rust C header exposes session active-tab consistency planning"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_retention_pruning_policy_json|Rust C header exposes retention pruning policy"
  "crates/actro-core/include/actro_core.h|actro_core_browser_storage_retention_pruning_plan_json|Rust C header exposes retention pruning planning"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cycle_request_plan_json|Rust browser storage sync owns cycle request planning"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_cycle_transport_result_json|Rust browser storage sync owns transport result classification"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_status_policy_json|Rust browser storage sync owns coordinator status/retry policy"
  "crates/actro-core/src/browser_data.rs|retryDelaySeconds|Rust browser storage sync status policy includes deterministic retry delay"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_schedule_state_json|Rust browser storage sync owns coordinator schedule state"
  "crates/actro-core/src/browser_data.rs|nextRunAtUnixSeconds|Rust browser storage sync schedule state includes next run timing"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_scheduler_status_json|Rust browser storage sync owns product-visible scheduler status"
  "crates/actro-core/src/browser_data.rs|merge_browser_storage_sync_cursors|Rust browser storage sync merges pull cursors into ack plans"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_ack_plan_json|Rust browser storage sync owns ack execution plans"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_remote_apply_plan_json|Rust browser storage sync owns remote apply execution plans"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_remote_apply_result_json|Rust browser storage sync owns remote apply result summaries"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_server_merge_plan_json|Rust browser storage sync owns reference merge execution plans"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_server_merge_result_json|Rust browser storage sync owns reference merge result summaries"
  "crates/actro-core/src/browser_data.rs|BrowserStorageSyncServerSql|Rust browser storage sync owns reference SQL contract"
  "crates/actro-core/src/browser_data.rs|browser_sync_server_revisions|Rust browser storage sync owns reference revision allocation schema"
  "crates/actro-core/src/browser_data.rs|allocate_server_revision|Rust browser storage sync owns reference revision allocation SQL"
  "crates/actro-core/src/browser_data.rs|merge_pushed_history_page|Rust browser storage sync owns reference merge SQL"
  "crates/actro-core/src/browser_data.rs|select_remote_history_pages|Rust browser storage sync owns reference remote pull SQL"
  "crates/actro-core/src/browser_data.rs|server_revision_param_rule|Rust browser storage sync documents reference merge binding rule"
  "crates/actro-sync-server/Cargo.toml|actro-core|Rust browser sync reference executor depends on core contracts"
  "crates/actro-sync-server/src/lib.rs|BrowserSyncServerDatabase|Rust browser sync reference executor has a concrete SQLite database boundary"
  "crates/actro-sync-server/src/lib.rs|merge_push_request_json|Rust browser sync reference executor applies pushed batches"
  "crates/actro-sync-server/src/lib.rs|pull_remote_batch_json|Rust browser sync reference executor builds pull batches"
  "crates/actro-sync-server/src/lib.rs|sync_cycle_json|Rust browser sync reference executor supports push-pull cycles"
  "crates/actro-sync-server/src/lib.rs|remoteBatch.*cursors|Rust browser sync reference tests require remote pull cursor handoff"
  "crates/actro-sync-server/src/lib.rs|BrowserSyncService|Rust browser sync reference has an endpoint-level service boundary"
  "crates/actro-sync-server/src/lib.rs|register_device|Rust browser sync reference service keeps local device bootstrap fixtures"
  "crates/actro-sync-server/src/lib.rs|authorize_context|Rust browser sync reference service enforces device authorization"
  "crates/actro-sync-server/src/lib.rs|max_push_records|Rust browser sync reference service enforces push record limits"
  "crates/actro-sync-server/src/lib.rs|BrowserSyncHttpRequest|Rust browser sync reference service has an HTTP facade request type"
  "crates/actro-sync-server/src/lib.rs|BrowserSyncHttpResponse|Rust browser sync reference service has an HTTP facade response type"
  "crates/actro-sync-server/src/lib.rs|handle_http_request|Rust browser sync reference service maps HTTP requests to service methods"
  "crates/actro-sync-server/src/lib.rs|BROWSER_STORAGE_SYNC_TRANSPORT_CYCLE_PATH|Rust browser sync reference HTTP facade uses core endpoint constants"
  "crates/actro-sync-server/src/lib.rs|http_facade_routes_contract_push_cycle_pull_and_cursors|Rust browser sync reference HTTP facade tests cover happy paths"
  "crates/actro-sync-server/src/lib.rs|http_facade_maps_auth_validation_and_routing_errors|Rust browser sync reference HTTP facade tests cover error mapping"
  "crates/actro-sync-server/Cargo.toml|tiny_http|Rust browser sync reference has a minimal HTTP runtime dependency"
  "crates/actro-sync-server/src/bin/actro-sync-server.rs|Server::http|Rust browser sync reference binary starts tiny_http socket runtime"
  "crates/actro-sync-server/src/bin/actro-sync-server.rs|handle_http_request|Rust browser sync reference binary delegates protocol handling to service facade"
  "crates/actro-sync-server/src/bin/actro-sync-server.rs|register-device|Rust browser sync reference binary supports local device bootstrap"
  "crates/actro-sync-server/src/bin/actro-sync-server.rs|parses_server_options|Rust browser sync reference binary tests CLI parsing"
  "package.json|cargo test -p actro-core -p actro-vault -p actro-sync-server|Rust verification includes sync reference executor"
  "crates/actro-core/src/browser_data.rs|BrowserStorageDataModelContract|Rust browser storage owns a data model contract"
  "crates/actro-core/src/browser_data.rs|browser_storage_data_model_json|Rust browser storage exports the data model contract as JSON"
  "crates/actro-core/src/browser_data.rs|local_primary_key_columns|Rust browser storage data model records local primary keys"
  "crates/actro-core/src/browser_data.rs|stable_identity_fields|Rust browser storage data model records stable sync identities"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_conflict_policy_json|Rust browser storage sync owns conflict policy contract"
  "crates/actro-core/src/browser_data.rs|browser_storage_sync_merge_policy_json|Rust browser storage sync owns collection merge policy contract"
  "crates/actro-core/src/browser_data.rs|browser_storage_address_canonicalization_policy_json|Rust browser storage sync owns address canonicalization policy contract"
  "crates/actro-core/src/browser_data.rs|browser_storage_retention_pruning_policy_json|Rust browser storage sync owns retention pruning policy contract"
  "crates/actro-core/src/browser_data.rs|browser_storage_retention_pruning_plan_json|Rust browser storage sync owns batched retention pruning planning"
  "crates/actro-core/src/browser_data.rs|consistency_reason|Rust browser storage sync owns session active-tab consistency diagnostics"
  "crates/actro-core/src/browser_data.rs|consistency_summary|Rust browser storage sync owns session active-tab consistency summaries"
  "crates/actro-core/src/browser_data.rs|retention_reason|Rust browser storage sync owns retention pruning row diagnostics"
  "crates/actro-core/src/browser_data.rs|retention_summary|Rust browser storage sync owns retention pruning summaries"
  "crates/actro-core/src/browser_data.rs|originDeviceHistoryPageLimitExceeded|Rust browser storage sync owns per-origin-device history pruning"
  "crates/actro-core/src/browser_data.rs|originDeviceSessionTabLimitExceeded|Rust browser storage sync owns per-origin-device session-tab pruning"
  "crates/actro-core/src/browser_data.rs|BROWSER_STORAGE_SYNC_REMOTE_APPLY_CONFLICT_POLICY|Rust browser storage sync names remote apply conflict policy"
  "crates/actro-core/src/browser_data.rs|BROWSER_STORAGE_SYNC_REMOTE_APPLY_CONFLICT_REASON|Rust browser storage sync names remote apply conflict reason"
  "crates/actro-core/src/browser_data.rs|BROWSER_STORAGE_SYNC_MERGE_POLICY_SESSION_TABS|Rust browser storage sync names session-tab merge policy"
  "crates/actro-core/src/browser_data.rs|apply_remote_history_page|Rust browser storage sync owns remote apply SQL"
  "crates/actro-core/src/browser_data.rs|mark_history_page_synced|Rust browser storage sync owns ack SQL"
  "crates/actro-core/src/browser_data.rs|sync_revision = MAX\\(sync_revision, \\?\\)|Rust browser storage ack SQL writes accepted sync revisions"
  "crates/actro-core/src/browser_data.rs|upsert_sync_cursor|Rust browser storage sync owns cursor ack SQL"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|\"syncRevision\": 999|Apple browser storage tests verify ack revision writeback"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|\\.put\\(\"syncRevision\", 999\\)|Android browser storage tests verify ack revision writeback"
  "apps/windows/actro.tests/Program.cs|\"syncRevision\": 999|Windows browser storage tests verify ack revision writeback"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|normalizeBrowserStorageSyncBatchJson|Apple ActroCore binding exposes browser sync batch normalization"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncBatchFromDirtyRowsJson|Apple ActroCore binding exposes dirty-row batch conversion"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncPushRequestFromDirtyRowsJson|Apple ActroCore binding exposes dirty-row push conversion"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|normalizeBrowserStorageSyncAckBatchJson|Apple ActroCore binding exposes browser sync ack normalization"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|normalizeBrowserStorageSyncCursorsJson|Apple ActroCore binding exposes browser sync cursor normalization"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncPushRequestJson|Apple ActroCore binding exposes browser sync push request building"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncPullRequestJson|Apple ActroCore binding exposes browser sync pull request building"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCycleResponseJson|Apple ActroCore binding exposes browser sync cycle response building"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|normalizeBrowserStorageSyncCycleResponseJson|Apple ActroCore binding exposes browser sync cycle response normalization"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncClientCyclePlanJson|Apple ActroCore binding exposes browser sync client cycle planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncClientAckBatchAfterRemoteApplyJson|Apple ActroCore binding exposes browser sync post-remote ack selection"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncTransportContractJson|Apple ActroCore binding exposes browser sync transport contract"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudStoreContractJson|Apple ActroCore binding exposes browser sync cloud store contract"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudProviderAdapterContractJson|Apple ActroCore binding exposes browser sync cloud provider adapter contract"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudProviderAdapterPlanJson|Apple ActroCore binding exposes browser sync cloud provider adapter plan"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudProviderErrorPolicyJson|Apple ActroCore binding exposes browser sync cloud provider error policy"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudProviderErrorClassificationJson|Apple ActroCore binding exposes browser sync cloud provider error classification"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|providerErrorClassificationJson|Apple ActroCore diagnostics binding accepts provider error classification"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudStoreCapabilityPreflightJson|Apple ActroCore binding exposes browser sync cloud store capability preflight"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudLocalFolderCapabilitiesJson|Apple ActroCore binding exposes browser sync local-folder capabilities"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudLocalFolderCapabilityPreflightJson|Apple ActroCore binding exposes browser sync local-folder capability preflight"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudLocalFolderCompatibilityPreflightJson|Apple ActroCore binding exposes browser sync local-folder compatibility preflight"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudDataModelJson|Apple ActroCore binding exposes browser sync cloud data model"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudCompatibilityStatusJson|Apple ActroCore binding exposes browser sync cloud compatibility status"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudCompatibilityPreflightJson|Apple ActroCore binding exposes browser sync cloud compatibility preflight"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudSchemaRolloutPlanJson|Apple ActroCore binding exposes browser sync cloud schema rollout planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudSchemaMigrationPlaybookJson|Apple ActroCore binding exposes browser sync cloud schema migration playbook"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSchemaMigrationPlanJson|Apple ActroCore binding exposes the browser storage schema migration plan"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageDatabase.swift|browserStorageSchemaMigrationPlanJson|Apple browser storage adapter consumes the Rust schema migration plan"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudRecoverySnapshotPlanJson|Apple ActroCore binding exposes browser sync cloud recovery snapshot planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudRecoverySnapshotResultPlanJson|Apple ActroCore binding exposes browser sync cloud recovery snapshot result handoff"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudLocalFolderRecoverySnapshotRoundJson|Apple ActroCore binding exposes browser sync local-folder recovery snapshot rounds"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudDefaultManifestJson|Apple ActroCore binding exposes browser sync cloud manifest defaults"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudLogObjectJson|Apple ActroCore binding exposes browser sync cloud log objects"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudSnapshotObjectJson|Apple ActroCore binding exposes browser sync cloud snapshot objects"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudMembershipPlanJson|Apple ActroCore binding exposes browser sync cloud membership plans"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudMembershipResultPlanJson|Apple ActroCore binding exposes browser sync cloud membership result plans"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudPushPlanJson|Apple ActroCore binding exposes browser sync cloud push planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudPushResultPlanJson|Apple ActroCore binding exposes browser sync cloud push result planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudPullPlanJson|Apple ActroCore binding exposes browser sync cloud pull planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudPullApplyPlanJson|Apple ActroCore binding exposes browser sync cloud pull apply planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudPullContinuationPlanJson|Apple ActroCore binding exposes browser sync cloud pull continuation planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudCompactionPlanJson|Apple ActroCore binding exposes browser sync cloud compaction planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudCompactionResultPlanJson|Apple ActroCore binding exposes browser sync cloud compaction result planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudLocalFolderCompactionRoundJson|Apple ActroCore binding exposes browser sync local-folder compaction round"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudLocalFolderCleanupRoundJson|Apple ActroCore binding exposes browser sync local-folder cleanup round"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudHighWatermarkPolicyJson|Apple ActroCore binding exposes browser sync cloud high-watermark policy"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudHighWatermarkPlanJson|Apple ActroCore binding exposes browser sync cloud high-watermark planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudCleanupResultPlanJson|Apple ActroCore binding exposes browser sync cloud cleanup result planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudCycleClientPlanJson|Apple ActroCore binding exposes browser sync cloud cycle client planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudCycleStatusPolicyJson|Apple ActroCore binding exposes browser sync cloud cycle status policy"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudMembershipStatusPolicyJson|Apple ActroCore binding exposes browser sync cloud membership status policy"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudCycleScheduleStateJson|Apple ActroCore binding exposes browser sync cloud cycle schedule state"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudMembershipScheduleStateJson|Apple ActroCore binding exposes browser sync cloud membership schedule state"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudMembershipAttemptPlanJson|Apple ActroCore binding exposes browser sync cloud membership attempt plan"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudMembershipLoopPlanJson|Apple ActroCore binding exposes browser sync cloud membership loop plan"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudProviderDiagnosticsJson|Apple ActroCore binding exposes browser sync cloud provider diagnostics"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudObservabilityEventJson|Apple ActroCore binding exposes browser sync cloud observability events"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudCycleAttemptPlanJson|Apple ActroCore binding exposes browser sync cloud cycle attempt plan"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudCycleLoopPlanJson|Apple ActroCore binding exposes browser sync cloud cycle loop plan"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudLocalFolderCycleRoundJson|Apple ActroCore binding exposes browser sync local-folder cycle rounds"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCloudLocalFolderMembershipRoundJson|Apple ActroCore binding exposes browser sync local-folder membership rounds"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCycleRequestPlanJson|Apple ActroCore binding exposes browser sync cycle request planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncCycleTransportResultJson|Apple ActroCore binding exposes browser sync transport result classification"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncStatusPolicyJson|Apple ActroCore binding exposes browser sync status policy"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncScheduleStateJson|Apple ActroCore binding exposes browser sync schedule state"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageDataModelJson|Apple ActroCore binding exposes browser storage data model"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncAckPlanJson|Apple ActroCore binding exposes browser sync ack planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncRemoteApplyPlanJson|Apple ActroCore binding exposes browser sync remote apply planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncRemoteApplyResultJson|Apple ActroCore binding exposes browser sync remote apply result summaries"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncServerMergePlanJson|Apple ActroCore binding exposes browser sync reference merge planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncServerMergeResultJson|Apple ActroCore binding exposes browser sync reference merge result summaries"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncConflictPolicyJson|Apple ActroCore binding exposes browser sync conflict policy"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSyncMergePolicyJson|Apple ActroCore binding exposes browser sync merge policy"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageAddressCanonicalizationPolicyJson|Apple ActroCore binding exposes address canonicalization policy"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageCanonicalizeAddressesJson|Apple ActroCore binding exposes batched address canonicalization"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageBookmarkFolderTreeIntegrityPolicyJson|Apple ActroCore binding exposes bookmark folder tree integrity policy"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageBookmarkFolderTreeIntegrityPlanJson|Apple ActroCore binding exposes bookmark folder tree integrity planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageBookmarkRestorePlacementPolicyJson|Apple ActroCore binding exposes bookmark restore placement policy"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageBookmarkRestorePlacementPlanJson|Apple ActroCore binding exposes bookmark restore placement planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageBookmarkOwnershipPolicyJson|Apple ActroCore binding exposes bookmark ownership policy"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageBookmarkOwnershipPlanJson|Apple ActroCore binding exposes bookmark ownership planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageHistoryVisitAggregationPolicyJson|Apple ActroCore binding exposes history visit aggregation policy"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageHistoryVisitAggregationPlanJson|Apple ActroCore binding exposes history visit aggregation planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSessionTabOrderingPolicyJson|Apple ActroCore binding exposes session tab ordering policy"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSessionTabOrderingPlanJson|Apple ActroCore binding exposes session tab ordering planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSessionActiveTabConsistencyPolicyJson|Apple ActroCore binding exposes session active-tab consistency policy"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageSessionActiveTabConsistencyPlanJson|Apple ActroCore binding exposes session active-tab consistency planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageRetentionPruningPolicyJson|Apple ActroCore binding exposes retention pruning policy"
  "platform/apple/packages/ActroCore/Sources/ActroCore/actro_core.swift|browserStorageRetentionPruningPlanJson|Apple ActroCore binding exposes retention pruning planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleSQLiteDatabase.swift|executeReturningChanges|Apple SQLite adapter reports rows affected for browser sync conflicts"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageDatabase.swift|applyRemoteSyncPlan|Apple browser storage adapter executes Rust browser sync remote apply plans"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageDatabase.swift|browserStorageSessionTabIdPlanJson|Apple browser storage adapter derives session tab ids through the Rust batch plan"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/database/BrowserStorageDatabase.kt|browserStorageSessionTabIdPlanJson|Android browser storage adapter derives session tab ids through the Rust batch plan"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageDatabase.swift|browserSessionSnapshotNormalizeJson|Apple browser storage adapter persists sessions through the Rust snapshot normalizer"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/database/BrowserStorageDatabase.kt|browserSessionSnapshotNormalizeJson|Android browser storage adapter persists sessions through the Rust snapshot normalizer"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|testSaveSessionAdoptsCoreNormalizedSnapshot|Apple browser storage tests cover Rust-normalized session snapshot persistence"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|saveSessionAdoptsCoreNormalizedSnapshot|Android instrumentation tests cover Rust-normalized session snapshot persistence"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageDatabase.swift|applySyncAckPlan|Apple browser storage adapter executes Rust browser sync ack plans"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageDatabase.swift|applySyncCycleResponse|Apple browser storage adapter applies Rust-planned sync cycles"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageDatabase.swift|exportDirtySyncPushRequest|Apple browser storage adapter exports dirty rows through Rust push builder"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageDatabase.swift|exportSyncCycleRequestPlan|Apple browser storage adapter exports Rust-planned sync cycle requests"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageDatabase.swift|applyCloudCycleClientPlan|Apple browser storage adapter applies Rust cloud cycle client plans"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageDatabase.swift|owner_kind|Apple browser storage adapter migrates bookmark owner kind"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageDatabase.swift|ownerKind|Apple browser storage adapter maps bookmark owner kind"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageDatabase.swift|surface_kind|Apple browser storage adapter migrates session surface kind"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageDatabase.swift|surfaceKind|Apple browser storage adapter maps session surface kind"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|AppleBrowserStorageSyncTransport|Apple browser sync client uses injectable transport"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|browserStorageSyncCycleTransportResultJson|Apple browser sync client classifies transport through Rust"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|runLocalFolderSyncCycle|Apple browser sync client exposes local-folder cloud cycle execution"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|runLocalFolderMembershipRound|Apple browser sync client exposes local-folder cloud membership execution"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|browserStorageSyncCloudLocalFolderCapabilityPreflightJson|Apple browser sync client consumes Rust local-folder capability preflight"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|browserStorageSyncCloudLocalFolderCompatibilityPreflightJson|Apple browser sync client consumes Rust local-folder compatibility preflight"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|compatibilityPreflight|Apple browser sync client returns Rust local-folder compatibility preflight"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|browserStorageSyncCloudLocalFolderCycleRoundJson|Apple browser sync client consumes Rust local-folder cycle helper"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|browserStorageSyncCloudLocalFolderCompactionRoundJson|Apple browser sync client consumes Rust local-folder compaction helper"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|browserStorageSyncCloudLocalFolderCleanupRoundJson|Apple browser sync client consumes Rust local-folder cleanup helper"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|appleBrowserStorageCloudSchemaRolloutPlan|Apple browser sync client consumes Rust schema rollout planning on compatibility block"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|appleBrowserStorageCloudSchemaMigrationPlaybook|Apple browser sync client consumes Rust schema migration playbook on compatibility block"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|appleBrowserStorageLocalFolderRecoverySnapshotRound|Apple browser sync client consumes Rust local-folder recovery snapshot round on compatibility block"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|browserStorageSyncCloudLocalFolderMembershipRoundJson|Apple browser sync client consumes Rust local-folder membership helper"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|appleBrowserStorageCloudMembershipStatusPolicy|Apple browser sync membership client consumes Rust membership status policy"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|\"statusPolicy\": statusPolicy|Apple browser sync membership client returns Rust membership status policy"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|appleBrowserStorageCloudMembershipScheduleState|Apple browser sync membership client consumes Rust membership schedule state"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|\"scheduleState\": scheduleState|Apple browser sync membership client returns Rust membership schedule state"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|appleBrowserStorageCloudMembershipAttemptPlan|Apple browser sync membership client consumes Rust membership attempt plan"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|\"attemptPlan\": attemptPlan|Apple browser sync membership client returns Rust membership attempt plan"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|appleBrowserStorageCloudMembershipLoopPlan|Apple browser sync membership client consumes Rust membership loop plan"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|\"loopPlan\": loopPlan|Apple browser sync membership client returns Rust membership loop plan"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|let schemaRolloutPlan: [String: Any]?|Apple browser sync membership client prepares Rust schema rollout on compatibility block"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|membershipRound[\"compatibilityPreflight\"]|Apple browser sync client lifts Rust membership compatibility preflight"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|observabilityEvent: observabilityEvent|Apple browser sync membership client exposes Rust observability events"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|browserStorageSyncCloudCycleScheduleStateJson|Apple browser sync client consumes Rust cloud cycle schedule state"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|browserStorageSyncCloudProviderDiagnosticsJson|Apple browser sync client consumes Rust cloud provider diagnostics"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|providerErrorClassification|Apple browser sync client passes provider error diagnostics slot"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|appleBrowserStorageLocalFolderProviderErrorClassification|Apple browser sync client builds provider error classification from real local-folder rounds"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|localFolderProviderRoundErrors|Apple browser sync client sources provider errors from local-folder rounds"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|browserStorageSyncCloudObservabilityEventJson|Apple browser sync client consumes Rust cloud observability events"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|browserStorageSyncCloudCycleAttemptPlanJson|Apple browser sync client consumes Rust cloud cycle attempt planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift|browserStorageSyncCloudCycleLoopPlanJson|Apple browser sync client consumes Rust cloud cycle loop planning"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncCoordinator.swift|AppleBrowserStorageSyncCredentialsProvider|Apple browser sync coordinator uses injectable credential lookup"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncCoordinator.swift|AppleBrowserStorageSyncClient|Apple browser sync coordinator delegates network/apply work to the sync client"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncCoordinator.swift|statusPolicy|Apple browser sync coordinator returns Rust-owned status policy payloads"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncCoordinator.swift|scheduleState|Apple browser sync coordinator returns Rust-owned schedule state payloads"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncScheduler.swift|AppleBrowserStorageSyncSchedulerSnapshot|Apple browser sync scheduler exposes platform schedule snapshots"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncScheduler.swift|nextRunDelaySeconds|Apple browser sync scheduler consumes Rust-owned schedule state timing"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncScheduler.swift|browserStorageSyncSchedulerStatusJson|Apple browser sync scheduler delegates product status to Rust"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncScheduler.swift|scheduleAfterLocalChange|Apple browser sync scheduler can debounce local browser data changes"
  "apps/ios/actro/Actro/Database/BrowserSyncCredentialsProvider.swift|keychainSessionAccount|iOS browser sync credentials come only from the stored Keychain sync session"
  "apps/ios/actro/Actro/Database/BrowserSyncCredentialsProvider.swift|AppleBrowserStorageSyncCredentialsProvider|iOS browser sync credentials implement the Apple coordinator provider"
  "apps/ios/actro/Actro/Database/BrowserDataRepository.swift|syncScheduler|iOS browser data repository wires the sync scheduler"
  "apps/ios/actro/Actro/Database/BrowserDataRepository.swift|syncStatusJson|iOS browser data repository exposes Rust-owned sync status"
  "apps/ios/actro/Actro/Database/BrowserDataRepository.swift|scheduleSyncAfterLocalChange|iOS browser data repository schedules local-change sync"
  "apps/ios/actro/Actro/App/BrowserDataModule.swift|BrowserDataModule|iOS exposes browser data through a full native module"
  "apps/ios/actro/Actro/App/BrowserDataModule.swift|loadRecentHistory|iOS BrowserDataModule exposes browser history reads"
  "apps/ios/actro/Actro/App/BrowserDataModule.swift|getBookmarks|iOS BrowserDataModule exposes browser bookmark reads"
  "apps/ios/actro/Actro/Database/BrowserDataRepository.swift|loadBookmarkPayloads|iOS browser data repository maps bookmark payloads behind the repository boundary"
  "apps/ios/actro/Actro/Database/BrowserDataRepository.swift|ownerKind: \"tab\"|iOS tab-derived bookmarks are marked with tab ownership"
  "apps/ios/actro/Actro/Database/BrowserDataRepository.swift|ownerId: tab.id.uuidString|iOS tab-derived bookmarks carry stable tab owner ids"
  "apps/ios/actro/Actro/App/BrowserDataModule.swift|saveSyncSession|iOS BrowserDataModule exposes sync credential writes"
  "apps/ios/actro/Actro/App/BrowserDataModule.swift|syncStatusJson|iOS BrowserDataModule exposes Rust-owned sync status"
  "apps/ios/actro/Actro/App/BrowserDataModule.m|RCT_EXTERN_MODULE\\(BrowserDataModule|iOS BrowserDataModule is registered with React Native"
  "apps/macos/actro/Actro/Settings/Sync/BrowserSyncCredentialsProvider.swift|save\\(baseUrl|macOS browser sync credentials have a local write surface for future account UI"
  "apps/macos/actro/Actro/Settings/Sync/BrowserSyncCredentialsProvider.swift|saveSession|macOS browser sync credentials persist account sync sessions"
  "apps/macos/actro/Actro/Settings/Sync/BrowserSyncCredentialsProvider.swift|legacyDeviceIdKey|macOS browser sync credentials migrate legacy device identity"
  "apps/macos/actro/Actro/Settings/Sync/BrowserSyncCredentialsProvider.swift|AppleBrowserStorageSyncCredentialsProvider|macOS browser sync credentials implement the Apple coordinator provider"
  "apps/macos/actro/Actro/Database/BrowserDataRepository.swift|syncScheduler|macOS browser data repository wires the sync scheduler"
  "apps/macos/actro/Actro/Database/BrowserDataRepository.swift|syncStatusJson|macOS browser data repository exposes Rust-owned sync status"
  "apps/macos/actro/Actro/Database/BrowserDataRepository.swift|scheduleSyncAfterLocalChange|macOS browser data repository schedules local-change sync"
  "apps/macos/actro/Actro/Database/BrowserDataRepository.swift|scheduleSyncAfterCredentialChange|macOS browser data repository schedules credential-change sync"
  "apps/macos/actro/Actro/Settings/SyncSettingsView.swift|BrowserDataSyncSettingsModel|macOS sync settings shows browser sync status and manual trigger"
  "apps/macos/actro/Actro/Settings/SyncSettingsView.swift|Save Connection|macOS sync settings exposes browser sync credential controls"
  "platform/apple/packages/ActroCore/Package.swift|ActroCoreTests|Apple ActroCore package wires browser storage migration tests"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|testMigratesBrowserStorageSQLiteV1V2V3ToV6|Apple tests cover browser storage v1/v2/v3 SQLite migration"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|testCloudProviderAdapterObjectSpacePlanContract|Apple tests cover provider adapter object-space plan"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|testCloudProviderConnectionStatusPolicyContract|Apple tests cover provider connection status policy"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|testCloudProviderErrorClassificationPolicyContract|Apple tests cover provider error classification"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|providerErrorClassification|Apple tests cover provider error diagnostics source mapping"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|batchHasBookmarkOwner|Apple tests cover browser bookmark owner sync payloads"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|exportDirtySyncPushRequest|Apple browser storage migration tests cover dirty push export"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|applySyncCycleResponse|Apple browser storage migration tests cover sync cycle apply"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|exportSyncCycleRequestPlan|Apple browser storage migration tests cover sync cycle request planning"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|testSyncClientExecutesInjectedCycleTransportAndAppliesResponse|Apple tests cover browser sync client cycle"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|testSyncClientRunsLocalFolderCloudCycleAndAppliesClientPlan|Apple tests cover local-folder cloud sync client"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|testSyncClientRunsLocalFolderCloudMembershipRound|Apple tests cover local-folder cloud membership client"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|finishCloudDeviceRevoked|Apple local-folder cloud membership test asserts Rust revoke classification"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|capabilityPreflight|Apple local-folder cloud sync test asserts Rust capability preflight"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|compatibilityPreflight|Apple local-folder cloud sync test asserts Rust compatibility preflight"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|clientUpgradeRequired|Apple local-folder cloud sync test asserts compatibility blocking"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|cloudCycleScheduleState|Apple local-folder cloud sync test asserts Rust schedule state"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|providerDiagnostics|Apple local-folder cloud sync test asserts Rust provider diagnostics"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|cloudObservabilityEvent|Apple local-folder cloud sync test asserts Rust observability event"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|loopPlan|Apple local-folder cloud sync test asserts Rust loop planning"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|compactionRound|Apple local-folder cloud sync test asserts Rust compaction round"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|cleanupRound|Apple local-folder cloud sync test asserts Rust cleanup round"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|recoverySnapshotRound|Apple local-folder cloud sync test asserts Rust recovery snapshot round"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|providerDiagnostics.*compatibilityClassification|Apple local-folder cloud membership test asserts Rust provider diagnostics"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|observabilityEvent.*planType|Apple local-folder cloud membership test asserts Rust observability event"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|cloudMembershipStatusPolicy|Apple local-folder cloud membership test asserts Rust membership status policy"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|cloudMembershipScheduleState|Apple local-folder cloud membership test asserts Rust membership schedule state"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|finishMembershipAttemptCount|Apple local-folder cloud membership test asserts Rust membership loop summary"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|cloudMembershipAttemptPlan|Apple local-folder cloud membership test asserts Rust membership attempt plan"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|cloudMembershipLoopPlan|Apple local-folder cloud membership test asserts Rust membership loop plan"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|blockedMembershipRollout|Apple local-folder cloud membership test asserts Rust schema rollout on compatibility block"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|testSyncCoordinatorSkipsCycleWhenCredentialsUnavailable|Apple tests cover browser sync credential gating"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|testSyncCoordinatorRunsClientWithAvailableCredentials|Apple tests cover browser sync coordinator delegation"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|waitForCredentials|Apple tests cover sync coordinator status policy"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|testSyncCoordinatorSchedulesRetryFromStatusPolicy|Apple tests cover sync coordinator schedule state"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|testSyncSchedulerConsumesScheduleStateAndResetsAfterSuccess|Apple tests cover sync scheduler retry lifecycle"
  "platform/apple/packages/ActroCore/Tests/ActroCoreTests/AppleBrowserStorageDatabaseTests.swift|retryScheduled|Apple tests cover Rust-owned sync scheduler status"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|normalizeBrowserStorageSyncBatchJson|Android ActroRust facade exposes browser sync batch normalization"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncBatchFromDirtyRowsJson|Android ActroRust facade exposes dirty-row batch conversion"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncPushRequestFromDirtyRowsJson|Android ActroRust facade exposes dirty-row push conversion"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|normalizeBrowserStorageSyncAckBatchJson|Android ActroRust facade exposes browser sync ack normalization"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|normalizeBrowserStorageSyncCursorsJson|Android ActroRust facade exposes browser sync cursor normalization"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncPushRequestJson|Android ActroRust facade exposes browser sync push request building"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncPullRequestJson|Android ActroRust facade exposes browser sync pull request building"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCycleResponseJson|Android ActroRust facade exposes browser sync cycle response building"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncSchedulerStatusJson|Android ActroRust facade exposes browser sync scheduler status"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|normalizeBrowserStorageSyncCycleResponseJson|Android ActroRust facade exposes browser sync cycle response normalization"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncClientCyclePlanJson|Android ActroRust facade exposes browser sync client cycle planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncClientAckBatchAfterRemoteApplyJson|Android ActroRust facade exposes browser sync post-remote ack selection"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncTransportContractJson|Android ActroRust facade exposes browser sync transport contract"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudStoreContractJson|Android ActroRust facade exposes browser sync cloud store contract"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudProviderAdapterContractJson|Android ActroRust facade exposes browser sync cloud provider adapter contract"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudProviderAdapterPlanJson|Android ActroRust facade exposes browser sync cloud provider adapter plan"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudProviderErrorPolicyJson|Android ActroRust facade exposes browser sync cloud provider error policy"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudProviderErrorClassificationJson|Android ActroRust facade exposes browser sync cloud provider error classification"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|providerErrorClassificationJson|Android ActroRust diagnostics facade accepts provider error classification"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudStoreCapabilityPreflightJson|Android ActroRust facade exposes browser sync cloud store capability preflight"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudLocalFolderCapabilitiesJson|Android ActroRust facade exposes browser sync local-folder capabilities"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudLocalFolderCapabilityPreflightJson|Android ActroRust facade exposes browser sync local-folder capability preflight"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudLocalFolderCompatibilityPreflightJson|Android ActroRust facade exposes browser sync local-folder compatibility preflight"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudDataModelJson|Android ActroRust facade exposes browser sync cloud data model"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudCompatibilityStatusJson|Android ActroRust facade exposes browser sync cloud compatibility status"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudCompatibilityPreflightJson|Android ActroRust facade exposes browser sync cloud compatibility preflight"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudSchemaRolloutPlanJson|Android ActroRust facade exposes browser sync cloud schema rollout planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudSchemaMigrationPlaybookJson|Android ActroRust facade exposes browser sync cloud schema migration playbook"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSchemaMigrationPlanJson|Android ActroRust facade exposes the browser storage schema migration plan"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudRecoverySnapshotPlanJson|Android ActroRust facade exposes browser sync cloud recovery snapshot planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudRecoverySnapshotResultPlanJson|Android ActroRust facade exposes browser sync cloud recovery snapshot result handoff"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudLocalFolderRecoverySnapshotRoundJson|Android ActroRust facade exposes browser sync local-folder recovery snapshot rounds"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudDefaultManifestJson|Android ActroRust facade exposes browser sync cloud manifest defaults"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudLogObjectJson|Android ActroRust facade exposes browser sync cloud log objects"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudSnapshotObjectJson|Android ActroRust facade exposes browser sync cloud snapshot objects"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudMembershipPlanJson|Android ActroRust facade exposes browser sync cloud membership plans"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudMembershipResultPlanJson|Android ActroRust facade exposes browser sync cloud membership result plans"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudPushPlanJson|Android ActroRust facade exposes browser sync cloud push planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudPushResultPlanJson|Android ActroRust facade exposes browser sync cloud push result planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudPullPlanJson|Android ActroRust facade exposes browser sync cloud pull planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudPullApplyPlanJson|Android ActroRust facade exposes browser sync cloud pull apply planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudPullContinuationPlanJson|Android ActroRust facade exposes browser sync cloud pull continuation planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudCompactionPlanJson|Android ActroRust facade exposes browser sync cloud compaction planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudCompactionResultPlanJson|Android ActroRust facade exposes browser sync cloud compaction result planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudLocalFolderCompactionRoundJson|Android ActroRust facade exposes browser sync local-folder compaction round"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudLocalFolderCleanupRoundJson|Android ActroRust facade exposes browser sync local-folder cleanup round"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudHighWatermarkPolicyJson|Android ActroRust facade exposes browser sync cloud high-watermark policy"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudHighWatermarkPlanJson|Android ActroRust facade exposes browser sync cloud high-watermark planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudCleanupResultPlanJson|Android ActroRust facade exposes browser sync cloud cleanup result planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudCycleClientPlanJson|Android ActroRust facade exposes browser sync cloud cycle client planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudCycleStatusPolicyJson|Android ActroRust facade exposes browser sync cloud cycle status policy"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudMembershipStatusPolicyJson|Android ActroRust facade exposes browser sync cloud membership status policy"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudCycleScheduleStateJson|Android ActroRust facade exposes browser sync cloud cycle schedule state"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudMembershipScheduleStateJson|Android ActroRust facade exposes browser sync cloud membership schedule state"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudMembershipAttemptPlanJson|Android ActroRust facade exposes browser sync cloud membership attempt plan"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudMembershipLoopPlanJson|Android ActroRust facade exposes browser sync cloud membership loop plan"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudProviderDiagnosticsJson|Android ActroRust facade exposes browser sync cloud provider diagnostics"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudObservabilityEventJson|Android ActroRust facade exposes browser sync cloud observability events"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudCycleAttemptPlanJson|Android ActroRust facade exposes browser sync cloud cycle attempt plan"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudCycleLoopPlanJson|Android ActroRust facade exposes browser sync cloud cycle loop plan"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudLocalFolderCycleRoundJson|Android ActroRust facade exposes browser sync local-folder cycle rounds"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCloudLocalFolderMembershipRoundJson|Android ActroRust facade exposes browser sync local-folder membership rounds"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCycleRequestPlanJson|Android ActroRust facade exposes browser sync cycle request planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncCycleTransportResultJson|Android ActroRust facade exposes browser sync transport result classification"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncStatusPolicyJson|Android ActroRust facade exposes browser sync status policy"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncScheduleStateJson|Android ActroRust facade exposes browser sync schedule state"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageDataModelJson|Android ActroRust facade exposes browser storage data model"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncAckPlanJson|Android ActroRust facade exposes browser sync ack planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncRemoteApplyPlanJson|Android ActroRust facade exposes browser sync remote apply planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncRemoteApplyResultJson|Android ActroRust facade exposes browser sync remote apply result summaries"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncServerMergePlanJson|Android ActroRust facade exposes browser sync reference merge planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncServerMergeResultJson|Android ActroRust facade exposes browser sync reference merge result summaries"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncConflictPolicyJson|Android ActroRust facade exposes browser sync conflict policy"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSyncMergePolicyJson|Android ActroRust facade exposes browser sync merge policy"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageAddressCanonicalizationPolicyJson|Android ActroRust facade exposes address canonicalization policy"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageCanonicalizeAddressesJson|Android ActroRust facade exposes batched address canonicalization"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageBookmarkFolderTreeIntegrityPolicyJson|Android ActroRust facade exposes bookmark folder tree integrity policy"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageBookmarkFolderTreeIntegrityPlanJson|Android ActroRust facade exposes bookmark folder tree integrity planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageBookmarkRestorePlacementPolicyJson|Android ActroRust facade exposes bookmark restore placement policy"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageBookmarkRestorePlacementPlanJson|Android ActroRust facade exposes bookmark restore placement planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageBookmarkOwnershipPolicyJson|Android ActroRust facade exposes bookmark ownership policy"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageBookmarkOwnershipPlanJson|Android ActroRust facade exposes bookmark ownership planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageHistoryVisitAggregationPolicyJson|Android ActroRust facade exposes history visit aggregation policy"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageHistoryVisitAggregationPlanJson|Android ActroRust facade exposes history visit aggregation planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSessionTabOrderingPolicyJson|Android ActroRust facade exposes session tab ordering policy"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSessionTabOrderingPlanJson|Android ActroRust facade exposes session tab ordering planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSessionActiveTabConsistencyPolicyJson|Android ActroRust facade exposes session active-tab consistency policy"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageSessionActiveTabConsistencyPlanJson|Android ActroRust facade exposes session active-tab consistency planning"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageRetentionPruningPolicyJson|Android ActroRust facade exposes retention pruning policy"
  "packages/actro-native-kotlin/src/main/kotlin/io/actrium/actro/rust/ActroRust.kt|browserStorageRetentionPruningPlanJson|Android ActroRust facade exposes retention pruning planning"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageAddressCanonicalizationPolicyJson|Android UniFFI binding exposes address canonicalization policy"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageCanonicalizeAddressesJson|Android UniFFI binding exposes batched address canonicalization"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageBookmarkFolderTreeIntegrityPolicyJson|Android UniFFI binding exposes bookmark folder tree integrity policy"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageBookmarkFolderTreeIntegrityPlanJson|Android UniFFI binding exposes bookmark folder tree integrity planning"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageBookmarkRestorePlacementPolicyJson|Android UniFFI binding exposes bookmark restore placement policy"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageBookmarkRestorePlacementPlanJson|Android UniFFI binding exposes bookmark restore placement planning"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageBookmarkOwnershipPolicyJson|Android UniFFI binding exposes bookmark ownership policy"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageBookmarkOwnershipPlanJson|Android UniFFI binding exposes bookmark ownership planning"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageHistoryVisitAggregationPolicyJson|Android UniFFI binding exposes history visit aggregation policy"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageHistoryVisitAggregationPlanJson|Android UniFFI binding exposes history visit aggregation planning"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSessionTabOrderingPolicyJson|Android UniFFI binding exposes session tab ordering policy"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSessionTabOrderingPlanJson|Android UniFFI binding exposes session tab ordering planning"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSessionActiveTabConsistencyPolicyJson|Android UniFFI binding exposes session active-tab consistency policy"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSessionActiveTabConsistencyPlanJson|Android UniFFI binding exposes session active-tab consistency planning"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageRetentionPruningPolicyJson|Android UniFFI binding exposes retention pruning policy"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageRetentionPruningPlanJson|Android UniFFI binding exposes retention pruning planning"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudProviderAdapterContractJson|Android UniFFI binding exposes browser sync provider adapter contract"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudProviderAdapterPlanJson|Android UniFFI binding exposes browser sync provider adapter plan"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudSchemaRolloutPlanJson|Android UniFFI binding exposes browser sync cloud schema rollout planning"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudSchemaMigrationPlaybookJson|Android UniFFI binding exposes browser sync cloud schema migration playbook"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSchemaMigrationPlanJson|Android UniFFI binding exposes the browser storage schema migration plan"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudRecoverySnapshotPlanJson|Android UniFFI binding exposes browser sync cloud recovery snapshot planning"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudRecoverySnapshotResultPlanJson|Android UniFFI binding exposes browser sync cloud recovery snapshot result handoff"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudLocalFolderRecoverySnapshotRoundJson|Android UniFFI binding exposes browser sync local-folder recovery snapshot rounds"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudCycleLoopPlanJson|Android UniFFI binding exposes browser sync cloud cycle loop plan"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudProviderDiagnosticsJson|Android UniFFI binding exposes browser sync cloud provider diagnostics"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudProviderErrorPolicyJson|Android UniFFI binding exposes browser sync cloud provider error policy"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudProviderErrorClassificationJson|Android UniFFI binding exposes browser sync cloud provider error classification"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|providerErrorClassificationJson|Android UniFFI diagnostics binding accepts provider error classification"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudMembershipStatusPolicyJson|Android UniFFI binding exposes browser sync cloud membership status policy"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudMembershipScheduleStateJson|Android UniFFI binding exposes browser sync cloud membership schedule state"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudMembershipAttemptPlanJson|Android UniFFI binding exposes browser sync cloud membership attempt plan"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudMembershipLoopPlanJson|Android UniFFI binding exposes browser sync cloud membership loop plan"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudObservabilityEventJson|Android UniFFI binding exposes browser sync cloud observability events"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudHighWatermarkPolicyJson|Android UniFFI binding exposes browser sync cloud high-watermark policy"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudHighWatermarkPlanJson|Android UniFFI binding exposes browser sync cloud high-watermark planning"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudCleanupResultPlanJson|Android UniFFI binding exposes browser sync cloud cleanup result planning"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudLocalFolderCleanupRoundJson|Android UniFFI binding exposes browser sync local-folder cleanup round"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudLocalFolderCapabilitiesJson|Android UniFFI binding exposes browser sync local-folder capabilities"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudLocalFolderCapabilityPreflightJson|Android UniFFI binding exposes browser sync local-folder capability preflight"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudLocalFolderCompatibilityPreflightJson|Android UniFFI binding exposes browser sync local-folder compatibility preflight"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudLocalFolderCycleRoundJson|Android UniFFI binding exposes browser sync local-folder cycle rounds"
  "packages/actro-native-kotlin/src/main/kotlin/uniffi/actro_core/actro_core.kt|browserStorageSyncCloudLocalFolderMembershipRoundJson|Android UniFFI binding exposes browser sync local-folder membership rounds"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/database/BrowserStorageDatabase.kt|applyRemoteSyncPlan|Android browser storage adapter executes Rust browser sync remote apply plans"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/database/BrowserStorageDatabase.kt|applySyncAckPlan|Android browser storage adapter executes Rust browser sync ack plans"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/database/BrowserStorageDatabase.kt|applySyncCycleResponse|Android browser storage adapter applies Rust-planned sync cycles"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/database/BrowserStorageDatabase.kt|exportDirtySyncPushRequest|Android browser storage adapter exports dirty rows through Rust push builder"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/database/BrowserStorageDatabase.kt|exportSyncCycleRequestPlan|Android browser storage adapter exports Rust-planned sync cycle requests"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/database/BrowserStorageDatabase.kt|applyCloudCycleClientPlan|Android browser storage adapter applies Rust cloud cycle client plans"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/database/BrowserStorageDatabase.kt|createAtPath|Android browser storage adapter can open isolated fixture databases"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/database/BrowserStorageDatabase.kt|executeUpdateDelete|Android browser storage adapter reports rows affected for browser sync conflicts"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/database/BrowserStorageDatabase.kt|owner_kind|Android browser storage adapter migrates bookmark owner kind"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/database/BrowserStorageDatabase.kt|ownerKind|Android browser storage adapter maps bookmark owner kind"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/database/BrowserStorageDatabase.kt|surface_kind|Android browser storage adapter migrates session surface kind"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/database/BrowserStorageDatabase.kt|surfaceKind|Android browser storage adapter maps session surface kind"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|BrowserStorageSyncTransport|Android browser sync client uses injectable transport"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|browserStorageSyncCycleTransportResultJson|Android browser sync client classifies transport through Rust"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|runLocalFolderSyncCycle|Android browser sync client exposes local-folder cloud cycle execution"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|runLocalFolderMembershipRound|Android browser sync client exposes local-folder cloud membership execution"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|browserStorageSyncCloudLocalFolderCapabilityPreflightJson|Android browser sync client consumes Rust local-folder capability preflight"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|browserStorageSyncCloudLocalFolderCompatibilityPreflightJson|Android browser sync client consumes Rust local-folder compatibility preflight"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|compatibilityPreflight|Android browser sync client returns Rust local-folder compatibility preflight"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|browserStorageSyncCloudLocalFolderCycleRoundJson|Android browser sync client consumes Rust local-folder cycle helper"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|browserStorageSyncCloudLocalFolderCompactionRoundJson|Android browser sync client consumes Rust local-folder compaction helper"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|browserStorageSyncCloudLocalFolderCleanupRoundJson|Android browser sync client consumes Rust local-folder cleanup helper"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|cloudSchemaRolloutPlan|Android browser sync client consumes Rust schema rollout planning on compatibility block"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|cloudSchemaMigrationPlaybook|Android browser sync client consumes Rust schema migration playbook on compatibility block"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/database/BrowserStorageDatabase.kt|browserStorageSchemaMigrationPlanJson|Android browser storage adapter consumes the Rust schema migration plan"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|localFolderRecoverySnapshotRound|Android browser sync client consumes Rust local-folder recovery snapshot round on compatibility block"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|browserStorageSyncCloudLocalFolderMembershipRoundJson|Android browser sync client consumes Rust local-folder membership helper"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|cloudMembershipStatusPolicy|Android browser sync membership client consumes Rust membership status policy"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|\"statusPolicy\"|Android browser sync membership client returns Rust membership status policy"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|cloudMembershipScheduleState|Android browser sync membership client consumes Rust membership schedule state"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|\"scheduleState\"|Android browser sync membership client returns Rust membership schedule state"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|cloudMembershipAttemptPlan|Android browser sync membership client consumes Rust membership attempt plan"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|\"attemptPlan\"|Android browser sync membership client returns Rust membership attempt plan"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|cloudMembershipLoopPlan|Android browser sync membership client consumes Rust membership loop plan"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|\"loopPlan\"|Android browser sync membership client returns Rust membership loop plan"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|val schemaRolloutPlan: JSONObject?|Android browser sync membership client prepares Rust schema rollout on compatibility block"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|val compatibilityPreflight = membershipRound.optJSONObject|Android browser sync client lifts Rust membership compatibility preflight"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|observabilityEvent = cloudObservabilityEvent|Android browser sync membership client exposes Rust observability events"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|browserStorageSyncCloudCycleScheduleStateJson|Android browser sync client consumes Rust cloud cycle schedule state"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|browserStorageSyncCloudProviderDiagnosticsJson|Android browser sync client consumes Rust cloud provider diagnostics"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|providerErrorClassificationJson|Android browser sync client passes provider error diagnostics slot"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|localFolderProviderErrorClassification|Android browser sync client builds provider error classification from real local-folder rounds"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|localFolderProviderRoundErrors|Android browser sync client sources provider errors from local-folder rounds"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|browserStorageSyncCloudObservabilityEventJson|Android browser sync client consumes Rust cloud observability events"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|browserStorageSyncCloudCycleAttemptPlanJson|Android browser sync client consumes Rust cloud cycle attempt planning"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt|browserStorageSyncCloudCycleLoopPlanJson|Android browser sync client consumes Rust cloud cycle loop planning"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncCoordinator.kt|BrowserStorageSyncCredentialsProvider|Android browser sync coordinator uses injectable credential lookup"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncCoordinator.kt|BrowserStorageSyncClient|Android browser sync coordinator delegates network/apply work to the sync client"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncCoordinator.kt|statusPolicy|Android browser sync coordinator returns Rust-owned status policy payloads"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncCoordinator.kt|scheduleState|Android browser sync coordinator returns Rust-owned schedule state payloads"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncScheduler.kt|BrowserStorageSyncSchedulerSnapshot|Android browser sync scheduler exposes platform schedule snapshots"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncScheduler.kt|nextRunDelaySeconds|Android browser sync scheduler consumes Rust-owned schedule state timing"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncScheduler.kt|statusJson|Android browser sync scheduler exposes Rust-owned product status"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncScheduler.kt|scheduleAfterLocalChange|Android browser sync scheduler can debounce local browser data changes"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncCredentialStore.kt|SharedPreferences|Android browser sync credential store persists native session credentials"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/repository/BrowserDataRepository.kt|onLocalMutation|Android browser data repository exposes local-change sync hooks"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/repository/BrowserDataRepository.kt|ownerKind = \"tab\"|Android tab-derived bookmarks are marked with tab ownership"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/repository/BrowserDataRepository.kt|ownerId = activeModel.id|Android tab-derived bookmarks carry stable tab owner ids"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/core/BrowserDataModule.kt|saveSyncSession|Android BrowserDataModule exposes sync credential writes"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/core/BrowserDataModule.kt|syncNow|Android BrowserDataModule exposes a native sync trigger"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/core/BrowserDataModule.kt|getSyncStatus|Android BrowserDataModule exposes native sync status"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/MainActivity.kt|BrowserSyncStack|Android phone shell wires local-change sync scheduling through the shared sync stack"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/tablet/TabletViewModel.kt|BrowserSyncStack|Android tablet shell wires local-change sync scheduling through the shared sync stack"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserSyncStack.kt|scheduleAfterLocalChange|Android shared sync stack schedules sync after local changes"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserSyncStack.kt|BrowserStorageSyncScheduler|Android shared sync stack owns the single sync scheduler"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/generated/ActroAndroidPersistenceContract.kt|object Runtime|Android generated persistence contract includes runtime database constants"
  "apps/android/actro/android/app/src/main/java/io/actrium/actro/generated/ActroAndroidPersistenceContract.kt|SELECT_TOKEN_BY_ENV|Android generated persistence contract includes auth token queries"
  "packages/actro-runtime/src/libs/Sqlite.ts|RUNTIME_DATABASE_NAME|RN runtime SQLite database name comes from Rust-generated contract"
  "packages/actro-runtime/src/actr-host/generated/actroCoreContract.ts|RUNTIME_DATABASE_NAME = 'AppDatabase.db'|RN generated contract exports the runtime database name"
  "packages/actro-runtime/src/repository/AuthRepository.ts|saveBrowserSyncSession|RN auth token saves feed the native browser sync credential store"
  "packages/actro-runtime/src/repository/AuthRepository.ts|clearBrowserSyncSession|RN auth token clears remove native browser sync credentials"
  "packages/actro-runtime/src/native-bridge/browser-data/BrowserDataModule.ts|getBrowserSyncStatus|RN runtime exposes typed browser sync status"
  "packages/actro-runtime/src/native-bridge/browser-data/BrowserDataModule.ts|hasBrowserSyncModule|RN runtime gates sync UI by BrowserDataModule sync capability"
  "packages/actro-runtime/src/system-apps/HistoryApp.tsx|isBrowserPageUrl|RN History opens browser URLs through TabModule instead of legacy notification-only modules"
  "packages/actro-runtime/src/system-apps/bookmarks/useBookmarksPage.ts|loadBrowserBookmarks|RN Bookmarks consumes native browser bookmarks when available"
  "packages/actro-runtime/src/system-apps/bookmarks/useBookmarksPage.ts|deleteBrowserBookmark|RN Bookmarks deletes native browser bookmarks through BrowserDataModule"
  "packages/actro-runtime/src/system-apps/settings/SettingsMain.tsx|getBrowserSyncStatus|RN Settings shows browser sync status"
  "packages/actro-runtime/src/system-apps/settings/SettingsMain.tsx|hasBrowserSyncModule|RN Settings uses sync capability detection instead of full browser data detection"
  "packages/actro-runtime/src/system-apps/settings/SettingsMain.tsx|syncBrowserDataNow|RN Settings exposes manual browser sync"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|migratesBrowserStorageSQLiteV1V2V3ToV6|Android instrumentation tests cover browser storage v1/v2/v3 SQLite migration"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|plansCloudProviderAdapterObjectSpaceThroughRust|Android instrumentation tests cover provider adapter object-space plan"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|classifiesCloudProviderConnectionStatusThroughRust|Android instrumentation tests cover provider connection status policy"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|classifiesCloudProviderErrorsThroughRust|Android instrumentation tests cover provider error classification"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|providerErrorClassification|Android instrumentation tests cover provider error diagnostics source mapping"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|batchHasBookmarkOwner|Android instrumentation tests cover browser bookmark owner sync payloads"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|exportDirtySyncPushRequest|Android browser storage migration tests cover dirty push export"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|applySyncCycleResponse|Android browser storage migration tests cover sync cycle apply"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|exportSyncCycleRequestPlan|Android browser storage migration tests cover sync cycle request planning"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|syncClientExecutesInjectedCycleTransportAndAppliesResponse|Android instrumentation tests cover browser sync client cycle"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|syncClientRunsLocalFolderCloudCycleAndAppliesClientPlan|Android instrumentation tests cover local-folder cloud sync client"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|syncClientRunsLocalFolderCloudMembershipRound|Android instrumentation tests cover local-folder cloud membership client"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|finishCloudDeviceRevoked|Android local-folder cloud membership test asserts Rust revoke classification"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|device-android-2|Android local-folder cloud membership test asserts compatibility blocking prevents new device writes"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|capabilityPreflight|Android local-folder cloud sync test asserts Rust capability preflight"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|compatibilityPreflight|Android local-folder cloud sync test asserts Rust compatibility preflight"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|clientUpgradeRequired|Android local-folder cloud sync test asserts compatibility blocking"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|cloudCycleScheduleState|Android local-folder cloud sync test asserts Rust schedule state"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|providerDiagnostics|Android local-folder cloud sync test asserts Rust provider diagnostics"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|cloudObservabilityEvent|Android local-folder cloud sync test asserts Rust observability event"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|loopPlan|Android local-folder cloud sync test asserts Rust loop planning"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|compactionRound|Android local-folder cloud sync test asserts Rust compaction round"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|cleanupRound|Android local-folder cloud sync test asserts Rust cleanup round"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|recoverySnapshotRound|Android local-folder cloud sync test asserts Rust recovery snapshot round"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|providerDiagnostics.*compatibilityClassification|Android local-folder cloud membership test asserts Rust provider diagnostics"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|observabilityEvent.*planType|Android local-folder cloud membership test asserts Rust observability event"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|cloudMembershipStatusPolicy|Android local-folder cloud membership test asserts Rust membership status policy"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|cloudMembershipScheduleState|Android local-folder cloud membership test asserts Rust membership schedule state"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|cloudMembershipAttemptPlan|Android local-folder cloud membership test asserts Rust membership attempt plan"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|cloudMembershipLoopPlan|Android local-folder cloud membership test asserts Rust membership loop plan"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|finishMembershipAttemptCount|Android local-folder cloud membership test asserts Rust membership loop summary"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|blockedMembershipRollout|Android local-folder cloud membership test asserts Rust schema rollout on compatibility block"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|syncCoordinatorSkipsCycleWhenCredentialsUnavailable|Android instrumentation tests cover browser sync credential gating"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|syncCoordinatorRunsClientWithAvailableCredentials|Android instrumentation tests cover browser sync coordinator delegation"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|waitForCredentials|Android instrumentation tests cover sync coordinator status policy"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|syncCoordinatorSchedulesRetryFromStatusPolicy|Android instrumentation tests cover sync coordinator schedule state"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|syncSchedulerConsumesScheduleStateAndResetsAfterSuccess|Android instrumentation tests cover sync scheduler retry lifecycle"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|retryScheduled|Android instrumentation tests cover Rust-owned sync scheduler status"
  "apps/android/actro/android/app/src/androidTest/java/io/actrium/actro/data/database/BrowserStorageDatabaseInstrumentedTest.kt|syncCoordinatorReadsStoredNativeCredentials|Android instrumentation tests cover native browser sync credential-store integration"
)

for requirement in "${windows_browser_data_requirements[@]}"; do
  file="${requirement%%|*}"
  remainder="${requirement#*|}"
  required_pattern="${remainder%%|*}"
  description="${remainder##*|}"

  if [[ ! -f "$file" ]]; then
    print_failure "$description file is missing"
  elif ! rg -q "$required_pattern" "$file"; then
    print_failure "$description"
  else
    print_ok "$description"
  fi
done

windows_snapshot_post_rewrite="$(
  rg -n 'NormalizeSnapshotFallback\(normalized' \
    apps/windows/actro/src/BrowserHistoryStore.cs \
    apps/windows/actro/src/BrowserBookmarkStore.cs \
    apps/windows/actro/src/BrowserSessionStore.cs \
    2>/dev/null || true
)"

if [[ -n "$windows_snapshot_post_rewrite" ]]; then
  print_failure "Windows browser stores re-normalize Rust snapshot output with platform rules"
  printf '%s\n' "$windows_snapshot_post_rewrite"
else
  print_ok "Windows browser stores adopt Rust snapshot normalization output verbatim"
fi

platform_session_tab_id_generation="$(
  rg -n 'fnv1a|stable_session_tab_id|stableSessionTabId|StableSessionTabId' \
    platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageDatabase.swift \
    apps/android/actro/android/app/src/main/java/io/actrium/actro/data/database/BrowserStorageDatabase.kt \
    apps/windows/actro/src/BrowserSessionStore.cs \
    apps/windows/actro/src/BrowserStorageDatabase.cs \
    2>/dev/null || true
)"

if [[ -n "$platform_session_tab_id_generation" ]]; then
  print_failure "Platform session persistence re-implements the Rust stable session tab id derivation"
  printf '%s\n' "$platform_session_tab_id_generation"
else
  print_ok "Platform session persistence leaves stable session tab id derivation to the Rust core"
fi

local_folder_capability_duplicate_fields="$(
  rg -n 'supportsConditionalWrite|supportsConditionalDelete|supportsReadIfChanged|supportsPrefixList|supportsPagedList|revisionKind|conflictResolution' \
    apps/windows/actro/src/BrowserStorageSyncClient.cs \
    platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageSyncClient.swift \
    apps/android/actro/android/app/src/main/java/io/actrium/actro/data/sync/BrowserStorageSyncClient.kt \
    2>/dev/null || true
)"

if [[ -n "$local_folder_capability_duplicate_fields" ]]; then
  print_failure "Local-folder sync clients duplicate provider capability fields instead of using the Rust local-folder capability preflight helper"
  printf '%s\n' "$local_folder_capability_duplicate_fields"
else
  print_ok "Local-folder sync clients use the Rust-owned local-folder capability preflight helper"
fi

apple_manifest_reader_files=(
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleActrPackageManifestReader.swift"
  "apps/ios/actro/Actro/Models/ActrPackageManifestReader.swift"
  "apps/macos/actro/Actro/Models/ActrPackageManifestReader.swift"
)

missing_apple_manifest_files=()
for file in "${apple_manifest_reader_files[@]}"; do
  if [[ ! -f "$file" ]]; then
    missing_apple_manifest_files+=("$file")
  fi
done

if [[ "${#missing_apple_manifest_files[@]}" -ne 0 ]]; then
  print_failure "Apple ACTR manifest reader boundary files are missing"
  printf '%s\n' "${missing_apple_manifest_files[@]}"
else
  print_ok "Apple ACTR manifest reader boundary files exist"
fi

apple_manifest_reader_duplicates="$(
  rg -n 'CryptoKit|SHA256|parseActrManifest|createActrIntegrityPlan|evaluateContract|CoreError|findManifestJSON|resolvedACTRIntegrityFileURL' \
    apps/ios/actro/Actro/Models/ActrPackageManifestReader.swift \
    apps/macos/actro/Actro/Models/ActrPackageManifestReader.swift \
    2>/dev/null || true
)"

if [[ -n "$apple_manifest_reader_duplicates" ]]; then
  print_failure "Apple app-local ACTR manifest readers duplicate shared package-loading logic"
  printf '%s\n' "$apple_manifest_reader_duplicates"
else
  print_ok "Apple app-local ACTR manifest readers delegate package-loading logic"
fi

if ! rg -q 'AppleActrPackageManifestReader\.read' \
  apps/ios/actro/Actro/Models/ActrPackageManifestReader.swift \
  apps/macos/actro/Actro/Models/ActrPackageManifestReader.swift; then
  print_failure "Apple app-local ACTR manifest readers do not call the shared ActroCore helper"
else
  print_ok "Apple app-local ACTR manifest readers call the shared ActroCore helper"
fi

apple_sqlite_boundary_files=(
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleSQLiteDatabase.swift"
  "platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageDatabase.swift"
  "apps/ios/actro/Actro/Database/DatabaseManager.swift"
  "apps/macos/actro/Actro/Database/DatabaseManager.swift"
)

missing_apple_sqlite_files=()
for file in "${apple_sqlite_boundary_files[@]}"; do
  if [[ ! -f "$file" ]]; then
    missing_apple_sqlite_files+=("$file")
  fi
done

if [[ "${#missing_apple_sqlite_files[@]}" -ne 0 ]]; then
  print_failure "Apple SQLite boundary files are missing"
  printf '%s\n' "${missing_apple_sqlite_files[@]}"
else
  print_ok "Apple SQLite boundary files exist"
fi

apple_database_manager_duplicates="$(
  rg -n 'import SQLite3|sqlite3_|OpaquePointer|CoreSqlValueInfo.*sqliteValue' \
    apps/ios/actro/Actro/Database/DatabaseManager.swift \
    apps/macos/actro/Actro/Database/DatabaseManager.swift \
    2>/dev/null || true
)"

if [[ -n "$apple_database_manager_duplicates" ]]; then
  print_failure "Apple app-local DatabaseManager files duplicate shared SQLite glue"
  printf '%s\n' "$apple_database_manager_duplicates"
else
  print_ok "Apple app-local DatabaseManager files delegate SQLite glue"
fi

if ! rg -q 'AppleSQLiteDatabase' \
  apps/ios/actro/Actro/Database/DatabaseManager.swift \
  apps/macos/actro/Actro/Database/DatabaseManager.swift; then
  print_failure "Apple app-local DatabaseManager files do not call the shared SQLite helper"
else
  print_ok "Apple app-local DatabaseManager files call the shared SQLite helper"
fi

apple_browser_storage_file="platform/apple/packages/ActroCore/Sources/ActroCore/AppleBrowserStorageDatabase.swift"
if [[ ! -f "$apple_browser_storage_file" ]]; then
  print_failure "Apple shared browser storage adapter is missing"
elif ! rg -q 'browserStorageSchema\(\)' "$apple_browser_storage_file"; then
  print_failure "Apple shared browser storage adapter does not consume Rust storage schema"
elif ! rg -q 'browserStorageRepositorySql\(\)' "$apple_browser_storage_file"; then
  print_failure "Apple shared browser storage adapter does not consume Rust repository SQL"
elif ! rg -q 'AppleSQLiteDatabase' "$apple_browser_storage_file"; then
  print_failure "Apple shared browser storage adapter does not own SQLite execution through AppleSQLiteDatabase"
elif ! rg -q 'hasMetaFlag' "$apple_browser_storage_file" ||
  ! rg -q 'setMetaFlag' "$apple_browser_storage_file"; then
  print_failure "Apple shared browser storage adapter is missing migration meta flag helpers"
elif ! rg -q 'deleteBookmark\(id: String\)' "$apple_browser_storage_file"; then
  print_failure "Apple shared browser storage adapter is missing id-scoped bookmark deletion"
elif ! rg -q 'tab_id' "$apple_browser_storage_file" ||
  ! rg -q 'syncSql\.' "$apple_browser_storage_file" ||
  ! rg -q 'browser_sync_cursors' "$apple_browser_storage_file" ||
  ! rg -q 'migrateBrowserStorageSchemaIfNeeded' "$apple_browser_storage_file"; then
  print_failure "Apple shared browser storage adapter is missing browser session/sync migration readwrite"
else
  print_ok "Apple shared browser storage adapter consumes Rust schema through AppleSQLiteDatabase"
fi

macos_browser_data_repository_file="apps/macos/actro/Actro/Database/BrowserDataRepository.swift"
macos_browser_view_model_file="apps/macos/actro/Actro/Views/MainContentView/BrowserViewModel.swift"
if [[ ! -f "$macos_browser_data_repository_file" ]]; then
  print_failure "macOS browser data repository is missing"
elif ! rg -q 'AppleBrowserStorageDatabase' "$macos_browser_data_repository_file"; then
  print_failure "macOS browser data repository does not consume the shared Apple browser storage adapter"
elif ! rg -q 'recordHistoryVisit' "$macos_browser_data_repository_file"; then
  print_failure "macOS browser data repository is missing history visit browser storage calls"
elif ! rg -q 'upsertBookmarkItem' "$macos_browser_data_repository_file"; then
  print_failure "macOS browser data repository is missing bookmark browser storage calls"
elif ! rg -q 'saveSession' "$macos_browser_data_repository_file"; then
  print_failure "macOS browser data repository is missing session browser storage calls"
elif ! rg -q 'clearHistory' "$macos_browser_data_repository_file"; then
  print_failure "macOS browser data repository is missing history clearing calls"
elif ! rg -q 'clearBookmarks' "$macos_browser_data_repository_file"; then
  print_failure "macOS browser data repository is missing history/bookmark browser storage calls"
elif ! rg -q 'loadRecentHistory' "$macos_browser_data_repository_file"; then
  print_failure "macOS browser data repository is missing shared browser history readback"
elif ! rg -q 'loadBookmarks' "$macos_browser_data_repository_file"; then
  print_failure "macOS browser data repository is missing shared browser bookmark readback"
elif ! rg -q 'backfillHistoryIfNeeded' "$macos_browser_data_repository_file"; then
  print_failure "macOS browser data repository is missing local history backfill"
elif ! rg -q 'backfillBookmarksIfNeeded' "$macos_browser_data_repository_file"; then
  print_failure "macOS browser data repository is missing local bookmark backfill"
elif [[ ! -f "$macos_browser_view_model_file" ]]; then
  print_failure "macOS BrowserViewModel is missing"
elif ! rg -q 'persistBrowserPageSession' "$macos_browser_view_model_file"; then
  print_failure "macOS BrowserViewModel does not persist browser-page session snapshots"
elif ! rg -q 'BrowserDataRepository\.shared' \
  apps/macos/actro/Actro/ViewModels/HistoryViewModel.swift \
  apps/macos/actro/Actro/ViewModels/BookmarkViewModel.swift; then
  print_failure "macOS history/bookmark view models do not write through BrowserDataRepository"
elif ! rg -q 'loadRecentHistory' apps/macos/actro/Actro/ViewModels/HistoryViewModel.swift ||
  ! rg -q 'loadBookmarks' apps/macos/actro/Actro/ViewModels/BookmarkViewModel.swift; then
  print_failure "macOS history/bookmark view models do not read back shared browser data"
elif ! rg -q 'backfillSharedBrowserHistory' apps/macos/actro/Actro/ViewModels/HistoryViewModel.swift ||
  ! rg -q 'backfillSharedBrowserBookmarks' apps/macos/actro/Actro/ViewModels/BookmarkViewModel.swift; then
  print_failure "macOS history/bookmark view models do not backfill local browser-page data"
else
  print_ok "macOS browser history/bookmark/session data uses the shared browser storage adapter"
fi

ios_browser_data_repository_file="apps/ios/actro/Actro/Database/BrowserDataRepository.swift"
ios_browser_view_model_file="apps/ios/actro/Actro/Views/MainContentView/BrowserViewModel.swift"
if [[ ! -f "$ios_browser_data_repository_file" ]]; then
  print_failure "iOS browser data repository is missing"
elif ! rg -q 'AppleBrowserStorageDatabase' "$ios_browser_data_repository_file"; then
  print_failure "iOS browser data repository does not consume the shared Apple browser storage adapter"
elif ! rg -q 'recordHistoryVisit' "$ios_browser_data_repository_file"; then
  print_failure "iOS browser data repository is missing history visit browser storage calls"
elif rg -q 'database\?\.saveBookmarks\(items\)' "$ios_browser_data_repository_file"; then
  print_failure "iOS tab-derived bookmark persistence full-replaces browser bookmarks"
elif ! rg -q 'staleTabBookmarkIds' "$ios_browser_data_repository_file" ||
  ! rg -q 'upsertBookmarkItem' "$ios_browser_data_repository_file" ||
  ! rg -q 'deleteBookmark\(id: bookmarkId\)' "$ios_browser_data_repository_file"; then
  print_failure "iOS tab-derived bookmark persistence does not use id-scoped upsert/delete"
elif ! rg -q 'saveSession' "$ios_browser_data_repository_file"; then
  print_failure "iOS browser data repository is missing session browser storage calls"
elif ! rg -q 'loadSessionTabs' "$ios_browser_data_repository_file"; then
  print_failure "iOS browser data repository is missing shared browser session readback"
elif ! rg -q 'loadRecentHistory' "$ios_browser_data_repository_file"; then
  print_failure "iOS browser data repository is missing shared browser history readback"
elif ! rg -q 'backfillHistoryIfNeeded' "$ios_browser_data_repository_file"; then
  print_failure "iOS browser data repository is missing local history backfill"
elif [[ ! -f "$ios_browser_view_model_file" ]]; then
  print_failure "iOS BrowserViewModel is missing"
elif ! rg -q 'persistBrowserPageData' "$ios_browser_view_model_file"; then
  print_failure "iOS BrowserViewModel does not persist browser-page session/bookmark snapshots"
elif ! rg -q 'restoreSharedBrowserSessionIfNeeded' "$ios_browser_view_model_file"; then
  print_failure "iOS BrowserViewModel does not restore empty local tabs from shared browser session"
elif ! rg -q 'BrowserDataRepository\.shared' \
  apps/ios/actro/Actro/ViewModels/HistoryViewModel.swift \
  "$ios_browser_view_model_file"; then
  print_failure "iOS history/view model code does not write through BrowserDataRepository"
elif ! rg -q 'loadRecentHistory' apps/ios/actro/Actro/ViewModels/HistoryViewModel.swift; then
  print_failure "iOS history view model does not read back shared browser history"
elif ! rg -q 'backfillSharedBrowserHistory' apps/ios/actro/Actro/ViewModels/HistoryViewModel.swift; then
  print_failure "iOS history view model does not backfill local browser-page history"
else
  print_ok "iOS browser history/bookmark/session data uses the shared browser storage adapter"
fi

apple_app_browser_storage_duplicates="$(
  rg -n 'browser_history_pages|browser_history_visits|browser_bookmarks|browser_sessions|browser_session_tabs' \
    apps/ios/actro/Actro \
    apps/macos/actro/Actro \
    --glob '*.swift' \
    2>/dev/null || true
)"

if [[ -n "$apple_app_browser_storage_duplicates" ]]; then
  print_failure "Apple app code duplicates browser storage table names instead of using AppleBrowserStorageDatabase"
  printf '%s\n' "$apple_app_browser_storage_duplicates"
else
  print_ok "Apple app code does not duplicate browser storage table names"
fi

apple_database_parity_pairs=(
  "apps/ios/actro/Actro/Database/HistoryRepository.swift|apps/macos/actro/Actro/Database/HistoryRepository.swift"
  "apps/ios/actro/Actro/Database/TagRepository.swift|apps/macos/actro/Actro/Database/TagRepository.swift"
)

apple_database_parity_failures=()
for pair in "${apple_database_parity_pairs[@]}"; do
  ios_file="${pair%%|*}"
  macos_file="${pair##*|}"

  if [[ ! -f "$ios_file" || ! -f "$macos_file" ]]; then
    apple_database_parity_failures+=("$ios_file <-> $macos_file (missing file)")
    continue
  fi

  if ! cmp -s "$ios_file" "$macos_file"; then
    apple_database_parity_failures+=("$ios_file <-> $macos_file")
  fi
done

if [[ "${#apple_database_parity_failures[@]}" -ne 0 ]]; then
  print_failure "Apple app-local database repositories drifted; extract or update both sides deliberately"
  printf '%s\n' "${apple_database_parity_failures[@]}"
else
  print_ok "Apple app-local database repositories are parity-locked"
fi

asr_post_processor_logs="$(
  rg -n 'Log\.[[:alpha:]]+\(' \
    apps/android/actro/android/app/src/main/java/io/actrium/actro/core/asr/TextPostProcessor.kt \
    2>/dev/null || true
)"

if [[ -n "$asr_post_processor_logs" ]]; then
  print_failure "Android ASR text post processor logs recognized speech text"
  printf '%s\n' "$asr_post_processor_logs"
else
  print_ok "Android ASR text post processor avoids recognized-text logging"
fi

if [[ "$failures" -ne 0 ]]; then
  echo
  echo "Rust boundary hygiene failed with $failures issue(s)"
  exit 1
fi

echo
echo "Rust boundary hygiene passed"
