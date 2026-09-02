#ifndef ACTRIUM_CORE_H
#define ACTRIUM_CORE_H

#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

uint32_t actrium_core_c_api_version(void);
const char *actrium_core_product_name(void);
const char *actrium_core_supported_contract_name(void);
uint32_t actrium_core_supported_contract_version(void);
uint32_t actrium_core_schema_version(void);
uint32_t actrium_core_runtime_schema_version(void);
uint32_t actrium_core_browser_search_engine_count(void);
const char *actrium_core_browser_search_engine_default_id(void);
const char *actrium_core_browser_search_engine_id(uint32_t index);
const char *actrium_core_browser_search_engine_display_name(uint32_t index);
const char *actrium_core_browser_search_engine_url_prefix(uint32_t index);
const char *actrium_core_browser_search_engine_normalize_id(const char *engine_id);
size_t actrium_core_browser_search_url(
    const char *query,
    const char *engine_id,
    char *buffer,
    size_t buffer_len
);
uint32_t actrium_core_browser_data_schema_version(void);
uint32_t actrium_core_browser_storage_schema_version(void);
const char *actrium_core_browser_storage_database_name(void);
size_t actrium_core_browser_storage_schema_statements_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_schema_migration_plan_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_table_names_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_repository_sql_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_sql_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_data_model_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_conflict_policy_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_merge_policy_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_address_canonicalization_policy_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_canonicalize_addresses_json(
    const char *addresses_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_bookmark_folder_tree_integrity_policy_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_bookmark_folder_tree_integrity_plan_json(
    const char *folders_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_bookmark_restore_placement_policy_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_bookmark_restore_placement_plan_json(
    const char *bookmarks_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_bookmark_ownership_policy_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_bookmark_ownership_plan_json(
    const char *bookmarks_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_history_visit_aggregation_policy_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_history_visit_aggregation_plan_json(
    const char *visits_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_session_tab_ordering_policy_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_session_tab_ordering_plan_json(
    const char *tabs_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_session_tab_id_plan_json(
    const char *tabs_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_session_active_tab_consistency_policy_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_session_active_tab_consistency_plan_json(
    const char *sessions_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_retention_pruning_policy_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_retention_pruning_plan_json(
    const char *records_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_batch_normalize_json(
    const char *batch_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_batch_from_dirty_rows_json(
    const char *scope,
    const char *device_id,
    int64_t generated_at_unix_seconds,
    const char *rows_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_push_request_from_dirty_rows_json(
    const char *scope,
    const char *device_id,
    int64_t generated_at_unix_seconds,
    const char *rows_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_ack_batch_normalize_json(
    const char *ack_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cursors_normalize_json(
    const char *cursors_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_batch_empty_json(
    const char *scope,
    const char *device_id,
    int64_t generated_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_push_request_json(
    const char *batch_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_pull_request_json(
    const char *scope,
    const char *device_id,
    int64_t generated_at_unix_seconds,
    const char *cursors_json,
    uint32_t max_records_per_collection,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cycle_response_json(
    const char *remote_batch_json,
    const char *ack_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cycle_response_normalize_json(
    const char *response_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_client_cycle_plan_json(
    const char *response_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_client_ack_batch_after_remote_apply_json(
    const char *response_json,
    const char *remote_apply_result_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_client_cycle_apply_result_json(
    const char *response_json,
    const char *remote_apply_result_json,
    bool ack_applied,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_transport_contract_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_store_contract_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_provider_adapter_contract_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_provider_connection_contract_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_provider_error_policy_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_provider_adapter_plan_json(
    const char *adapter_snapshot_json,
    int64_t observed_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_store_capability_preflight_json(
    const char *capabilities_json,
    int64_t observed_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_provider_connection_status_policy_json(
    const char *provider_snapshot_json,
    uint32_t retry_attempt,
    int64_t observed_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_provider_error_classification_json(
    const char *provider_errors_json,
    uint32_t retry_attempt,
    int64_t observed_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_normalize_browser_sync_provider_settings_json(
    const char *settings_json,
    bool is_root_absolute,
    int64_t observed_at_unix_seconds,
    const char *fresh_device_id,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_local_folder_capabilities_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_local_folder_capability_preflight_json(
    int64_t observed_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_local_folder_compatibility_preflight_json(
    const char *root_dir,
    int64_t observed_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_data_model_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_compatibility_status_json(
    const char *document_kind,
    const char *document_json,
    int64_t observed_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_compatibility_preflight_json(
    const char *documents_json,
    int64_t observed_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_schema_rollout_plan_json(
    const char *compatibility_preflight_json,
    int64_t observed_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_schema_migration_playbook_json(
    const char *schema_rollout_plan_json,
    int64_t observed_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_recovery_snapshot_plan_json(
    const char *schema_migration_playbook_json,
    const char *compatibility_preflight_json,
    int64_t observed_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_recovery_snapshot_result_plan_json(
    const char *recovery_snapshot_plan_json,
    const char *write_results_json,
    int64_t generated_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_local_folder_recovery_snapshot_round_json(
    const char *root_dir,
    const char *schema_migration_playbook_json,
    const char *compatibility_preflight_json,
    int64_t observed_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_default_manifest_json(
    const char *scope,
    const char *device_id,
    int64_t generated_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_manifest_normalize_json(
    const char *manifest_json,
    const char *fallback_scope,
    const char *fallback_device_id,
    int64_t observed_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_device_normalize_json(
    const char *device_json,
    const char *fallback_scope,
    const char *fallback_device_id,
    int64_t observed_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_log_object_json(
    const char *collection,
    int64_t sequence,
    const char *batch_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_log_object_normalize_json(
    const char *log_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_snapshot_object_json(
    const char *collection,
    int64_t generated_at_unix_seconds,
    const char *batch_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_snapshot_object_normalize_json(
    const char *snapshot_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_membership_plan_json(
    const char *request_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_membership_result_plan_json(
    const char *membership_plan_json,
    const char *write_results_json,
    int64_t generated_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_local_folder_membership_round_json(
    const char *root_dir,
    const char *request_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_local_folder_compaction_round_json(
    const char *root_dir,
    const char *device_id,
    int64_t generated_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_local_folder_cleanup_round_json(
    const char *root_dir,
    const char *cursors_json,
    const char *device_id,
    int64_t generated_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_local_folder_recovery_retention_round_json(
    const char *root_dir,
    uint32_t retain_snapshots,
    int64_t generated_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_push_plan_json(
    const char *manifest_json,
    const char *manifest_revision,
    const char *device_json,
    const char *device_revision,
    const char *push_request_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_push_result_plan_json(
    const char *push_plan_json,
    const char *write_results_json,
    int64_t generated_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_pull_plan_json(
    const char *manifest_json,
    const char *device_id,
    const char *cursors_json,
    int64_t generated_at_unix_seconds,
    uint32_t max_log_objects_per_collection,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_pull_apply_plan_json(
    const char *manifest_json,
    const char *device_id,
    const char *cursors_json,
    const char *snapshots_json,
    const char *logs_json,
    int64_t generated_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_pull_continuation_plan_json(
    const char *pull_plan_json,
    const char *read_results_json,
    int64_t generated_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_high_watermark_policy_json(
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_high_watermark_plan_json(
    const char *manifest_json,
    const char *snapshots_json,
    const char *logs_json,
    const char *cursors_json,
    const char *device_id,
    int64_t generated_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_cleanup_result_plan_json(
    const char *high_watermark_plan_json,
    const char *delete_results_json,
    int64_t generated_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_compaction_plan_json(
    const char *manifest_json,
    const char *manifest_revision,
    const char *snapshots_json,
    const char *logs_json,
    const char *device_id,
    int64_t generated_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_compaction_result_plan_json(
    const char *compaction_plan_json,
    const char *write_results_json,
    const char *delete_results_json,
    int64_t generated_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_cycle_client_plan_json(
    const char *cycle_round_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_cycle_client_apply_result_json(
    const char *cycle_round_json,
    const char *step_results_json,
    const char *remote_apply_result_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_cycle_status_policy_json(
    const char *cycle_round_json,
    uint32_t retry_attempt,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_membership_status_policy_json(
    const char *membership_round_json,
    uint32_t retry_attempt,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_cycle_schedule_state_json(
    const char *status_policy_json,
    int64_t observed_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_membership_schedule_state_json(
    const char *status_policy_json,
    int64_t observed_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_provider_diagnostics_json(
    const char *capability_preflight_json,
    const char *compatibility_preflight_json,
    const char *provider_error_classification_json,
    const char *cycle_status_policy_json,
    const char *schedule_state_json,
    int64_t observed_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_observability_event_json(
    const char *provider_diagnostics_json,
    const char *attempt_plan_json,
    const char *loop_plan_json,
    int64_t observed_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_cycle_attempt_plan_json(
    const char *cycle_round_json,
    uint32_t retry_attempt,
    int64_t observed_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_membership_attempt_plan_json(
    const char *membership_round_json,
    uint32_t retry_attempt,
    int64_t observed_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_membership_loop_plan_json(
    const char *membership_rounds_json,
    uint32_t retry_attempt,
    int64_t observed_at_unix_seconds,
    uint32_t max_attempts,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_cycle_loop_plan_json(
    const char *cycle_rounds_json,
    uint32_t retry_attempt,
    int64_t observed_at_unix_seconds,
    uint32_t max_attempts,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cloud_local_folder_cycle_round_json(
    const char *root_dir,
    const char *push_request_json,
    const char *cursors_json,
    const char *device_id,
    int64_t generated_at_unix_seconds,
    uint32_t max_log_objects_per_collection,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cycle_request_plan_json(
    const char *base_url,
    const char *access_token,
    const char *push_request_json,
    const char *cursors_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_cycle_transport_result_json(
    int32_t status_code,
    const char *response_json,
    const char *error_message,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_status_policy_json(
    const char *classification,
    bool should_retry,
    const char *client_action,
    uint32_t retry_attempt,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_schedule_state_json(
    const char *status_policy_json,
    int64_t observed_at_unix_seconds,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_scheduler_status_json(
    const char *scheduler_snapshot_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_ack_plan_json(
    const char *ack_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_remote_apply_plan_json(
    const char *batch_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_remote_apply_result_json(
    const char *plan_json,
    const char *result_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_server_merge_plan_json(
    const char *push_request_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_storage_sync_server_merge_result_json(
    const char *plan_json,
    const char *result_json,
    char *buffer,
    size_t buffer_len
);
uint32_t actrium_core_browser_history_max_items(void);
uint32_t actrium_core_browser_bookmark_max_items(void);
uint32_t actrium_core_browser_session_max_tabs(void);
uint32_t actrium_core_browser_data_max_title_length(void);
const char *actrium_core_browser_home_address(void);
const char *actrium_core_browser_history_address(void);
const char *actrium_core_browser_bookmarks_address(void);
size_t actrium_core_browser_history_snapshot_normalize_json(
    const char *snapshot_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_bookmark_snapshot_normalize_json(
    const char *snapshot_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_session_snapshot_normalize_json(
    const char *snapshot_json,
    char *buffer,
    size_t buffer_len
);
size_t actrium_core_browser_session_home_snapshot_json(
    char *buffer,
    size_t buffer_len
);

#ifdef __cplusplus
}
#endif

#endif
