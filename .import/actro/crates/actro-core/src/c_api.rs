use core::ffi::c_char;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::sync::{Mutex, OnceLock, PoisonError};

use crate::{actr, auth, browser, browser_data, persistence, runtime_persistence, PRODUCT_NAME};

pub const ACTRO_CORE_C_API_VERSION: u32 = 1;

#[no_mangle]
pub extern "C" fn actro_core_c_api_version() -> u32 {
    ACTRO_CORE_C_API_VERSION
}

#[no_mangle]
pub extern "C" fn actro_core_product_name() -> *const c_char {
    c_str_ptr(PRODUCT_NAME)
}

#[no_mangle]
pub extern "C" fn actro_core_supported_contract_name() -> *const c_char {
    c_str_ptr(actr::SUPPORTED_CONTRACT_NAME)
}

#[no_mangle]
pub extern "C" fn actro_core_supported_contract_version() -> u32 {
    actr::SUPPORTED_CONTRACT_VERSION
}

#[no_mangle]
pub extern "C" fn actro_core_schema_version() -> u32 {
    persistence::CORE_SCHEMA_VERSION
}

#[no_mangle]
pub extern "C" fn actro_core_runtime_schema_version() -> u32 {
    runtime_persistence::RUNTIME_DB_SCHEMA_VERSION
}

#[no_mangle]
pub extern "C" fn actro_core_auth_environment_dev() -> *const c_char {
    c_str_ptr(auth::AUTH_ENVIRONMENT_DEV)
}

#[no_mangle]
pub extern "C" fn actro_core_auth_environment_beta() -> *const c_char {
    c_str_ptr(auth::AUTH_ENVIRONMENT_BETA)
}

#[no_mangle]
pub extern "C" fn actro_core_auth_environment_prod() -> *const c_char {
    c_str_ptr(auth::AUTH_ENVIRONMENT_PROD)
}

#[no_mangle]
pub extern "C" fn actro_core_auth_account_base_url(environment: *const c_char) -> *const c_char {
    let environment = c_char_to_string(environment).unwrap_or_default();
    c_str_ptr(auth::auth_account_base_url(&environment))
}

#[no_mangle]
pub extern "C" fn actro_core_browser_search_engine_count() -> u32 {
    browser::SEARCH_ENGINES.len() as u32
}

#[no_mangle]
pub extern "C" fn actro_core_browser_search_engine_default_id() -> *const c_char {
    c_str_ptr(browser::DEFAULT_SEARCH_ENGINE_ID)
}

#[no_mangle]
pub extern "C" fn actro_core_browser_search_engine_id(index: u32) -> *const c_char {
    browser::SEARCH_ENGINES
        .get(index as usize)
        .map(|engine| c_str_ptr(engine.id))
        .unwrap_or_else(|| c_str_ptr(""))
}

#[no_mangle]
pub extern "C" fn actro_core_browser_search_engine_display_name(index: u32) -> *const c_char {
    browser::SEARCH_ENGINES
        .get(index as usize)
        .map(|engine| c_str_ptr(engine.display_name))
        .unwrap_or_else(|| c_str_ptr(""))
}

#[no_mangle]
pub extern "C" fn actro_core_browser_search_engine_url_prefix(index: u32) -> *const c_char {
    browser::SEARCH_ENGINES
        .get(index as usize)
        .map(|engine| c_str_ptr(engine.search_url_prefix))
        .unwrap_or_else(|| c_str_ptr(""))
}

#[no_mangle]
pub extern "C" fn actro_core_browser_search_engine_normalize_id(
    engine_id: *const c_char,
) -> *const c_char {
    let engine_id = c_char_to_string(engine_id);
    c_str_ptr(browser::normalize_search_engine_id(engine_id.as_deref()))
}

#[no_mangle]
pub extern "C" fn actro_core_browser_search_url(
    query: *const c_char,
    engine_id: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let query = c_char_to_string(query).unwrap_or_default();
    let engine_id = c_char_to_string(engine_id);
    let url = browser::browser_search_url(&query, engine_id.as_deref());
    let bytes = url.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_data_schema_version() -> u32 {
    browser_data::BROWSER_DATA_SCHEMA_VERSION
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_schema_version() -> u32 {
    browser_data::BROWSER_STORAGE_SCHEMA_VERSION
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_database_name() -> *const c_char {
    c_str_ptr(browser_data::BROWSER_STORAGE_DATABASE_NAME)
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_schema_statements_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let statements = browser_data::browser_storage_schema_statements_json();
    let bytes = statements.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_schema_migration_plan_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let plan = browser_data::browser_storage_schema_migration_plan_json();
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_table_names_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let table_names = browser_data::browser_storage_table_names_json();
    let bytes = table_names.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_repository_sql_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let repository_sql = browser_data::browser_storage_repository_sql_json();
    let bytes = repository_sql.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_sql_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let sync_sql = browser_data::browser_storage_sync_sql_json();
    let bytes = sync_sql.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_data_model_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let data_model = browser_data::browser_storage_data_model_json();
    let bytes = data_model.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_conflict_policy_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let policy = browser_data::browser_storage_sync_conflict_policy_json();
    let bytes = policy.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_merge_policy_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let policy = browser_data::browser_storage_sync_merge_policy_json();
    let bytes = policy.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_address_canonicalization_policy_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let policy = browser_data::browser_storage_address_canonicalization_policy_json();
    let bytes = policy.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_canonicalize_addresses_json(
    addresses_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let addresses_json = c_char_to_string(addresses_json).unwrap_or_default();
    let result = browser_data::browser_storage_canonicalize_addresses_json(&addresses_json);
    let bytes = result.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_bookmark_folder_tree_integrity_policy_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let policy = browser_data::browser_storage_bookmark_folder_tree_integrity_policy_json();
    let bytes = policy.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_bookmark_folder_tree_integrity_plan_json(
    folders_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let folders_json = c_char_to_string(folders_json).unwrap_or_default();
    let result =
        browser_data::browser_storage_bookmark_folder_tree_integrity_plan_json(&folders_json);
    let bytes = result.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_bookmark_restore_placement_policy_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let policy = browser_data::browser_storage_bookmark_restore_placement_policy_json();
    let bytes = policy.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_bookmark_restore_placement_plan_json(
    bookmarks_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let bookmarks_json = c_char_to_string(bookmarks_json).unwrap_or_default();
    let result =
        browser_data::browser_storage_bookmark_restore_placement_plan_json(&bookmarks_json);
    let bytes = result.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_bookmark_ownership_policy_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let policy = browser_data::browser_storage_bookmark_ownership_policy_json();
    let bytes = policy.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_bookmark_ownership_plan_json(
    bookmarks_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let bookmarks_json = c_char_to_string(bookmarks_json).unwrap_or_default();
    let result = browser_data::browser_storage_bookmark_ownership_plan_json(&bookmarks_json);
    let bytes = result.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_history_visit_aggregation_policy_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let policy = browser_data::browser_storage_history_visit_aggregation_policy_json();
    let bytes = policy.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_history_visit_aggregation_plan_json(
    visits_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let visits_json = c_char_to_string(visits_json).unwrap_or_default();
    let result = browser_data::browser_storage_history_visit_aggregation_plan_json(&visits_json);
    let bytes = result.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_session_tab_ordering_policy_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let policy = browser_data::browser_storage_session_tab_ordering_policy_json();
    let bytes = policy.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_session_tab_ordering_plan_json(
    tabs_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let tabs_json = c_char_to_string(tabs_json).unwrap_or_default();
    let result = browser_data::browser_storage_session_tab_ordering_plan_json(&tabs_json);
    let bytes = result.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_session_tab_id_plan_json(
    tabs_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let tabs_json = c_char_to_string(tabs_json).unwrap_or_default();
    let result = browser_data::browser_storage_session_tab_id_plan_json(&tabs_json);
    let bytes = result.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_session_active_tab_consistency_policy_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let policy = browser_data::browser_storage_session_active_tab_consistency_policy_json();
    let bytes = policy.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_session_active_tab_consistency_plan_json(
    sessions_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let sessions_json = c_char_to_string(sessions_json).unwrap_or_default();
    let result =
        browser_data::browser_storage_session_active_tab_consistency_plan_json(&sessions_json);
    let bytes = result.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_retention_pruning_policy_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let policy = browser_data::browser_storage_retention_pruning_policy_json();
    let bytes = policy.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_retention_pruning_plan_json(
    records_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let records_json = c_char_to_string(records_json).unwrap_or_default();
    let result = browser_data::browser_storage_retention_pruning_plan_json(&records_json);
    let bytes = result.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_batch_normalize_json(
    batch_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let batch_json = c_char_to_string(batch_json).unwrap_or_default();
    let normalized = browser_data::normalize_browser_storage_sync_batch_json(&batch_json);
    let bytes = normalized.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_batch_from_dirty_rows_json(
    scope: *const c_char,
    device_id: *const c_char,
    generated_at_unix_seconds: i64,
    rows_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let scope = c_char_to_string(scope).unwrap_or_default();
    let device_id = c_char_to_string(device_id).unwrap_or_default();
    let rows_json = c_char_to_string(rows_json).unwrap_or_default();
    let batch = browser_data::browser_storage_sync_batch_from_dirty_rows_json(
        &scope,
        &device_id,
        generated_at_unix_seconds,
        &rows_json,
    );
    let bytes = batch.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_push_request_from_dirty_rows_json(
    scope: *const c_char,
    device_id: *const c_char,
    generated_at_unix_seconds: i64,
    rows_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let scope = c_char_to_string(scope).unwrap_or_default();
    let device_id = c_char_to_string(device_id).unwrap_or_default();
    let rows_json = c_char_to_string(rows_json).unwrap_or_default();
    let request = browser_data::browser_storage_sync_push_request_from_dirty_rows_json(
        &scope,
        &device_id,
        generated_at_unix_seconds,
        &rows_json,
    );
    let bytes = request.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_ack_batch_normalize_json(
    ack_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let ack_json = c_char_to_string(ack_json).unwrap_or_default();
    let normalized = browser_data::normalize_browser_storage_sync_ack_batch_json(&ack_json);
    let bytes = normalized.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cursors_normalize_json(
    cursors_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let cursors_json = c_char_to_string(cursors_json).unwrap_or_default();
    let normalized = browser_data::normalize_browser_storage_sync_cursors_json(&cursors_json);
    let bytes = normalized.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_batch_empty_json(
    scope: *const c_char,
    device_id: *const c_char,
    generated_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let scope = c_char_to_string(scope).unwrap_or_default();
    let device_id = c_char_to_string(device_id).unwrap_or_default();
    let empty = browser_data::empty_browser_storage_sync_batch_json(
        &scope,
        &device_id,
        generated_at_unix_seconds,
    );
    let bytes = empty.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_push_request_json(
    batch_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let batch_json = c_char_to_string(batch_json).unwrap_or_default();
    let request = browser_data::browser_storage_sync_push_request_json(&batch_json);
    let bytes = request.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_pull_request_json(
    scope: *const c_char,
    device_id: *const c_char,
    generated_at_unix_seconds: i64,
    cursors_json: *const c_char,
    max_records_per_collection: u32,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let scope = c_char_to_string(scope).unwrap_or_default();
    let device_id = c_char_to_string(device_id).unwrap_or_default();
    let cursors_json = c_char_to_string(cursors_json).unwrap_or_default();
    let request = browser_data::browser_storage_sync_pull_request_json(
        &scope,
        &device_id,
        generated_at_unix_seconds,
        &cursors_json,
        max_records_per_collection,
    );
    let bytes = request.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cycle_response_json(
    remote_batch_json: *const c_char,
    ack_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let remote_batch_json = c_char_to_string(remote_batch_json).unwrap_or_default();
    let ack_json = c_char_to_string(ack_json).unwrap_or_default();
    let response =
        browser_data::browser_storage_sync_cycle_response_json(&remote_batch_json, &ack_json);
    let bytes = response.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cycle_response_normalize_json(
    response_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let response_json = c_char_to_string(response_json).unwrap_or_default();
    let response = browser_data::normalize_browser_storage_sync_cycle_response_json(&response_json);
    let bytes = response.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_client_cycle_plan_json(
    response_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let response_json = c_char_to_string(response_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_client_cycle_plan_json(&response_json);
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_client_ack_batch_after_remote_apply_json(
    response_json: *const c_char,
    remote_apply_result_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let response_json = c_char_to_string(response_json).unwrap_or_default();
    let remote_apply_result_json = c_char_to_string(remote_apply_result_json).unwrap_or_default();
    let ack = browser_data::browser_storage_sync_client_ack_batch_after_remote_apply_json(
        &response_json,
        &remote_apply_result_json,
    );
    let bytes = ack.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_client_cycle_apply_result_json(
    response_json: *const c_char,
    remote_apply_result_json: *const c_char,
    ack_applied: bool,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let response_json = c_char_to_string(response_json).unwrap_or_default();
    let remote_apply_result_json = c_char_to_string(remote_apply_result_json).unwrap_or_default();
    let envelope = browser_data::browser_storage_sync_client_cycle_apply_result_json(
        &response_json,
        &remote_apply_result_json,
        ack_applied,
    );
    let bytes = envelope.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_transport_contract_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let contract = browser_data::browser_storage_sync_transport_contract_json();
    let bytes = contract.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_store_contract_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let contract = browser_data::browser_storage_sync_cloud_store_contract_json();
    let bytes = contract.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_provider_adapter_contract_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let contract = browser_data::browser_storage_sync_cloud_provider_adapter_contract_json();
    let bytes = contract.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_provider_connection_contract_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let contract = browser_data::browser_storage_sync_cloud_provider_connection_contract_json();
    let bytes = contract.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_provider_error_policy_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let policy = browser_data::browser_storage_sync_cloud_provider_error_policy_json();
    let bytes = policy.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_provider_adapter_plan_json(
    adapter_snapshot_json: *const c_char,
    observed_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let adapter_snapshot_json = c_char_to_string(adapter_snapshot_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_provider_adapter_plan_json(
        &adapter_snapshot_json,
        observed_at_unix_seconds,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_store_capability_preflight_json(
    capabilities_json: *const c_char,
    observed_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let capabilities_json = c_char_to_string(capabilities_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_store_capability_preflight_json(
        &capabilities_json,
        observed_at_unix_seconds,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_provider_connection_status_policy_json(
    provider_snapshot_json: *const c_char,
    retry_attempt: u32,
    observed_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let provider_snapshot_json = c_char_to_string(provider_snapshot_json).unwrap_or_default();
    let policy = browser_data::browser_storage_sync_cloud_provider_connection_status_policy_json(
        &provider_snapshot_json,
        retry_attempt,
        observed_at_unix_seconds,
    );
    let bytes = policy.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_normalize_browser_sync_provider_settings_json(
    settings_json: *const c_char,
    is_root_absolute: bool,
    observed_at_unix_seconds: i64,
    fresh_device_id: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let settings_json = c_char_to_string(settings_json).unwrap_or_default();
    let fresh_device_id = c_char_to_string(fresh_device_id).unwrap_or_default();
    let normalized = browser_data::normalize_browser_sync_provider_settings_json(
        &settings_json,
        is_root_absolute,
        observed_at_unix_seconds,
        &fresh_device_id,
    );
    let bytes = normalized.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_provider_error_classification_json(
    provider_errors_json: *const c_char,
    retry_attempt: u32,
    observed_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let provider_errors_json = c_char_to_string(provider_errors_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_provider_error_classification_json(
        &provider_errors_json,
        retry_attempt,
        observed_at_unix_seconds,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_local_folder_capabilities_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let capabilities = browser_data::browser_storage_sync_cloud_local_folder_capabilities_json();
    let bytes = capabilities.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_local_folder_capability_preflight_json(
    observed_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let plan = browser_data::browser_storage_sync_cloud_local_folder_capability_preflight_json(
        observed_at_unix_seconds,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_local_folder_compatibility_preflight_json(
    root_dir: *const c_char,
    observed_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let root_dir = c_char_to_string(root_dir).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_local_folder_compatibility_preflight_json(
        &root_dir,
        observed_at_unix_seconds,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_data_model_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let contract = browser_data::browser_storage_sync_cloud_data_model_json();
    let bytes = contract.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_default_manifest_json(
    scope: *const c_char,
    device_id: *const c_char,
    generated_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let scope = c_char_to_string(scope).unwrap_or_default();
    let device_id = c_char_to_string(device_id).unwrap_or_default();
    let manifest = browser_data::browser_storage_sync_cloud_default_manifest_json(
        &scope,
        &device_id,
        generated_at_unix_seconds,
    );
    let bytes = manifest.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_manifest_normalize_json(
    manifest_json: *const c_char,
    fallback_scope: *const c_char,
    fallback_device_id: *const c_char,
    observed_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let manifest_json = c_char_to_string(manifest_json).unwrap_or_default();
    let fallback_scope = c_char_to_string(fallback_scope).unwrap_or_default();
    let fallback_device_id = c_char_to_string(fallback_device_id).unwrap_or_default();
    let manifest = browser_data::normalize_browser_storage_sync_cloud_manifest_json(
        &manifest_json,
        &fallback_scope,
        &fallback_device_id,
        observed_at_unix_seconds,
    );
    let bytes = manifest.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_device_normalize_json(
    device_json: *const c_char,
    fallback_scope: *const c_char,
    fallback_device_id: *const c_char,
    observed_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let device_json = c_char_to_string(device_json).unwrap_or_default();
    let fallback_scope = c_char_to_string(fallback_scope).unwrap_or_default();
    let fallback_device_id = c_char_to_string(fallback_device_id).unwrap_or_default();
    let device = browser_data::normalize_browser_storage_sync_cloud_device_json(
        &device_json,
        &fallback_scope,
        &fallback_device_id,
        observed_at_unix_seconds,
    );
    let bytes = device.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_log_object_json(
    collection: *const c_char,
    sequence: i64,
    batch_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let collection = c_char_to_string(collection).unwrap_or_default();
    let batch_json = c_char_to_string(batch_json).unwrap_or_default();
    let log = browser_data::browser_storage_sync_cloud_log_object_json(
        &collection,
        sequence,
        &batch_json,
    );
    let bytes = log.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_log_object_normalize_json(
    log_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let log_json = c_char_to_string(log_json).unwrap_or_default();
    let log = browser_data::normalize_browser_storage_sync_cloud_log_object_json(&log_json);
    let bytes = log.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_snapshot_object_json(
    collection: *const c_char,
    generated_at_unix_seconds: i64,
    batch_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let collection = c_char_to_string(collection).unwrap_or_default();
    let batch_json = c_char_to_string(batch_json).unwrap_or_default();
    let snapshot = browser_data::browser_storage_sync_cloud_snapshot_object_json(
        &collection,
        generated_at_unix_seconds,
        &batch_json,
    );
    let bytes = snapshot.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_snapshot_object_normalize_json(
    snapshot_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let snapshot_json = c_char_to_string(snapshot_json).unwrap_or_default();
    let snapshot =
        browser_data::normalize_browser_storage_sync_cloud_snapshot_object_json(&snapshot_json);
    let bytes = snapshot.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_compatibility_status_json(
    document_kind: *const c_char,
    document_json: *const c_char,
    observed_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let document_kind = c_char_to_string(document_kind).unwrap_or_default();
    let document_json = c_char_to_string(document_json).unwrap_or_default();
    let status = browser_data::browser_storage_sync_cloud_compatibility_status_json(
        &document_kind,
        &document_json,
        observed_at_unix_seconds,
    );
    let bytes = status.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_compatibility_preflight_json(
    documents_json: *const c_char,
    observed_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let documents_json = c_char_to_string(documents_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_compatibility_preflight_json(
        &documents_json,
        observed_at_unix_seconds,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_schema_rollout_plan_json(
    compatibility_preflight_json: *const c_char,
    observed_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let compatibility_preflight_json =
        c_char_to_string(compatibility_preflight_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_schema_rollout_plan_json(
        &compatibility_preflight_json,
        observed_at_unix_seconds,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_schema_migration_playbook_json(
    schema_rollout_plan_json: *const c_char,
    observed_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let schema_rollout_plan_json = c_char_to_string(schema_rollout_plan_json).unwrap_or_default();
    let playbook = browser_data::browser_storage_sync_cloud_schema_migration_playbook_json(
        &schema_rollout_plan_json,
        observed_at_unix_seconds,
    );
    let bytes = playbook.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_recovery_snapshot_plan_json(
    schema_migration_playbook_json: *const c_char,
    compatibility_preflight_json: *const c_char,
    observed_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let schema_migration_playbook_json =
        c_char_to_string(schema_migration_playbook_json).unwrap_or_default();
    let compatibility_preflight_json =
        c_char_to_string(compatibility_preflight_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_recovery_snapshot_plan_json(
        &schema_migration_playbook_json,
        &compatibility_preflight_json,
        observed_at_unix_seconds,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_recovery_snapshot_result_plan_json(
    recovery_snapshot_plan_json: *const c_char,
    write_results_json: *const c_char,
    generated_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let recovery_snapshot_plan_json =
        c_char_to_string(recovery_snapshot_plan_json).unwrap_or_default();
    let write_results_json = c_char_to_string(write_results_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_recovery_snapshot_result_plan_json(
        &recovery_snapshot_plan_json,
        &write_results_json,
        generated_at_unix_seconds,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_local_folder_recovery_snapshot_round_json(
    root_dir: *const c_char,
    schema_migration_playbook_json: *const c_char,
    compatibility_preflight_json: *const c_char,
    observed_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let root_dir = c_char_to_string(root_dir).unwrap_or_default();
    let schema_migration_playbook_json =
        c_char_to_string(schema_migration_playbook_json).unwrap_or_default();
    let compatibility_preflight_json =
        c_char_to_string(compatibility_preflight_json).unwrap_or_default();
    let round = browser_data::browser_storage_sync_cloud_local_folder_recovery_snapshot_round_json(
        &root_dir,
        &schema_migration_playbook_json,
        &compatibility_preflight_json,
        observed_at_unix_seconds,
    );
    let bytes = round.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_membership_plan_json(
    request_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let request_json = c_char_to_string(request_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_membership_plan_json(&request_json);
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_membership_result_plan_json(
    membership_plan_json: *const c_char,
    write_results_json: *const c_char,
    generated_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let membership_plan_json = c_char_to_string(membership_plan_json).unwrap_or_default();
    let write_results_json = c_char_to_string(write_results_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_membership_result_plan_json(
        &membership_plan_json,
        &write_results_json,
        generated_at_unix_seconds,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_local_folder_membership_round_json(
    root_dir: *const c_char,
    request_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let root_dir = c_char_to_string(root_dir).unwrap_or_default();
    let request_json = c_char_to_string(request_json).unwrap_or_default();
    let round = browser_data::browser_storage_sync_cloud_local_folder_membership_round_json(
        &root_dir,
        &request_json,
    );
    let bytes = round.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_local_folder_compaction_round_json(
    root_dir: *const c_char,
    device_id: *const c_char,
    generated_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let root_dir = c_char_to_string(root_dir).unwrap_or_default();
    let device_id = c_char_to_string(device_id).unwrap_or_default();
    let round = browser_data::browser_storage_sync_cloud_local_folder_compaction_round_json(
        &root_dir,
        &device_id,
        generated_at_unix_seconds,
    );
    let bytes = round.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_local_folder_cleanup_round_json(
    root_dir: *const c_char,
    cursors_json: *const c_char,
    device_id: *const c_char,
    generated_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let root_dir = c_char_to_string(root_dir).unwrap_or_default();
    let cursors_json = c_char_to_string(cursors_json).unwrap_or_default();
    let device_id = c_char_to_string(device_id).unwrap_or_default();
    let round = browser_data::browser_storage_sync_cloud_local_folder_cleanup_round_json(
        &root_dir,
        &cursors_json,
        &device_id,
        generated_at_unix_seconds,
    );
    let bytes = round.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_local_folder_recovery_retention_round_json(
    root_dir: *const c_char,
    retain_snapshots: u32,
    generated_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let root_dir = c_char_to_string(root_dir).unwrap_or_default();
    let round = browser_data::browser_storage_sync_cloud_local_folder_recovery_retention_round_json(
        &root_dir,
        retain_snapshots,
        generated_at_unix_seconds,
    );
    let bytes = round.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_push_plan_json(
    manifest_json: *const c_char,
    manifest_revision: *const c_char,
    device_json: *const c_char,
    device_revision: *const c_char,
    push_request_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let manifest_json = c_char_to_string(manifest_json).unwrap_or_default();
    let manifest_revision = c_char_to_string(manifest_revision).unwrap_or_default();
    let device_json = c_char_to_string(device_json).unwrap_or_default();
    let device_revision = c_char_to_string(device_revision).unwrap_or_default();
    let push_request_json = c_char_to_string(push_request_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_push_plan_json(
        &manifest_json,
        &manifest_revision,
        &device_json,
        &device_revision,
        &push_request_json,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_push_result_plan_json(
    push_plan_json: *const c_char,
    write_results_json: *const c_char,
    generated_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let push_plan_json = c_char_to_string(push_plan_json).unwrap_or_default();
    let write_results_json = c_char_to_string(write_results_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_push_result_plan_json(
        &push_plan_json,
        &write_results_json,
        generated_at_unix_seconds,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_pull_plan_json(
    manifest_json: *const c_char,
    device_id: *const c_char,
    cursors_json: *const c_char,
    generated_at_unix_seconds: i64,
    max_log_objects_per_collection: u32,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let manifest_json = c_char_to_string(manifest_json).unwrap_or_default();
    let device_id = c_char_to_string(device_id).unwrap_or_default();
    let cursors_json = c_char_to_string(cursors_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_pull_plan_json(
        &manifest_json,
        &device_id,
        &cursors_json,
        generated_at_unix_seconds,
        max_log_objects_per_collection,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_pull_apply_plan_json(
    manifest_json: *const c_char,
    device_id: *const c_char,
    cursors_json: *const c_char,
    snapshots_json: *const c_char,
    logs_json: *const c_char,
    generated_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let manifest_json = c_char_to_string(manifest_json).unwrap_or_default();
    let device_id = c_char_to_string(device_id).unwrap_or_default();
    let cursors_json = c_char_to_string(cursors_json).unwrap_or_default();
    let snapshots_json = c_char_to_string(snapshots_json).unwrap_or_default();
    let logs_json = c_char_to_string(logs_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_pull_apply_plan_json(
        &manifest_json,
        &device_id,
        &cursors_json,
        &snapshots_json,
        &logs_json,
        generated_at_unix_seconds,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_pull_continuation_plan_json(
    pull_plan_json: *const c_char,
    read_results_json: *const c_char,
    generated_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let pull_plan_json = c_char_to_string(pull_plan_json).unwrap_or_default();
    let read_results_json = c_char_to_string(read_results_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_pull_continuation_plan_json(
        &pull_plan_json,
        &read_results_json,
        generated_at_unix_seconds,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_high_watermark_policy_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let policy = browser_data::browser_storage_sync_cloud_high_watermark_policy_json();
    let bytes = policy.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_high_watermark_plan_json(
    manifest_json: *const c_char,
    snapshots_json: *const c_char,
    logs_json: *const c_char,
    cursors_json: *const c_char,
    device_id: *const c_char,
    generated_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let manifest_json = c_char_to_string(manifest_json).unwrap_or_default();
    let snapshots_json = c_char_to_string(snapshots_json).unwrap_or_default();
    let logs_json = c_char_to_string(logs_json).unwrap_or_default();
    let cursors_json = c_char_to_string(cursors_json).unwrap_or_default();
    let device_id = c_char_to_string(device_id).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_high_watermark_plan_json(
        &manifest_json,
        &snapshots_json,
        &logs_json,
        &cursors_json,
        &device_id,
        generated_at_unix_seconds,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_cleanup_result_plan_json(
    high_watermark_plan_json: *const c_char,
    delete_results_json: *const c_char,
    generated_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let high_watermark_plan_json = c_char_to_string(high_watermark_plan_json).unwrap_or_default();
    let delete_results_json = c_char_to_string(delete_results_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_cleanup_result_plan_json(
        &high_watermark_plan_json,
        &delete_results_json,
        generated_at_unix_seconds,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_compaction_plan_json(
    manifest_json: *const c_char,
    manifest_revision: *const c_char,
    snapshots_json: *const c_char,
    logs_json: *const c_char,
    device_id: *const c_char,
    generated_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let manifest_json = c_char_to_string(manifest_json).unwrap_or_default();
    let manifest_revision = c_char_to_string(manifest_revision).unwrap_or_default();
    let snapshots_json = c_char_to_string(snapshots_json).unwrap_or_default();
    let logs_json = c_char_to_string(logs_json).unwrap_or_default();
    let device_id = c_char_to_string(device_id).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_compaction_plan_json(
        &manifest_json,
        &manifest_revision,
        &snapshots_json,
        &logs_json,
        &device_id,
        generated_at_unix_seconds,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_compaction_result_plan_json(
    compaction_plan_json: *const c_char,
    write_results_json: *const c_char,
    delete_results_json: *const c_char,
    generated_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let compaction_plan_json = c_char_to_string(compaction_plan_json).unwrap_or_default();
    let write_results_json = c_char_to_string(write_results_json).unwrap_or_default();
    let delete_results_json = c_char_to_string(delete_results_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_compaction_result_plan_json(
        &compaction_plan_json,
        &write_results_json,
        &delete_results_json,
        generated_at_unix_seconds,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_cycle_client_plan_json(
    cycle_round_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let cycle_round_json = c_char_to_string(cycle_round_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_cycle_client_plan_json(&cycle_round_json);
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_cycle_client_apply_result_json(
    cycle_round_json: *const c_char,
    step_results_json: *const c_char,
    remote_apply_result_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let cycle_round_json = c_char_to_string(cycle_round_json).unwrap_or_default();
    let step_results_json = c_char_to_string(step_results_json).unwrap_or_default();
    let remote_apply_result_json = c_char_to_string(remote_apply_result_json).unwrap_or_default();
    let envelope = browser_data::browser_storage_sync_cloud_cycle_client_apply_result_json(
        &cycle_round_json,
        &step_results_json,
        &remote_apply_result_json,
    );
    let bytes = envelope.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_cycle_status_policy_json(
    cycle_round_json: *const c_char,
    retry_attempt: u32,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let cycle_round_json = c_char_to_string(cycle_round_json).unwrap_or_default();
    let policy = browser_data::browser_storage_sync_cloud_cycle_status_policy_json(
        &cycle_round_json,
        retry_attempt,
    );
    let bytes = policy.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_membership_status_policy_json(
    membership_round_json: *const c_char,
    retry_attempt: u32,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let membership_round_json = c_char_to_string(membership_round_json).unwrap_or_default();
    let policy = browser_data::browser_storage_sync_cloud_membership_status_policy_json(
        &membership_round_json,
        retry_attempt,
    );
    let bytes = policy.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_cycle_schedule_state_json(
    status_policy_json: *const c_char,
    observed_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let status_policy_json = c_char_to_string(status_policy_json).unwrap_or_default();
    let state = browser_data::browser_storage_sync_cloud_cycle_schedule_state_json(
        &status_policy_json,
        observed_at_unix_seconds,
    );
    let bytes = state.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_membership_schedule_state_json(
    status_policy_json: *const c_char,
    observed_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let status_policy_json = c_char_to_string(status_policy_json).unwrap_or_default();
    let state = browser_data::browser_storage_sync_cloud_membership_schedule_state_json(
        &status_policy_json,
        observed_at_unix_seconds,
    );
    let bytes = state.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_provider_diagnostics_json(
    capability_preflight_json: *const c_char,
    compatibility_preflight_json: *const c_char,
    provider_error_classification_json: *const c_char,
    cycle_status_policy_json: *const c_char,
    schedule_state_json: *const c_char,
    observed_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let capability_preflight_json = c_char_to_string(capability_preflight_json).unwrap_or_default();
    let compatibility_preflight_json =
        c_char_to_string(compatibility_preflight_json).unwrap_or_default();
    let provider_error_classification_json =
        c_char_to_string(provider_error_classification_json).unwrap_or_default();
    let cycle_status_policy_json = c_char_to_string(cycle_status_policy_json).unwrap_or_default();
    let schedule_state_json = c_char_to_string(schedule_state_json).unwrap_or_default();
    let diagnostics = browser_data::browser_storage_sync_cloud_provider_diagnostics_json(
        &capability_preflight_json,
        &compatibility_preflight_json,
        &provider_error_classification_json,
        &cycle_status_policy_json,
        &schedule_state_json,
        observed_at_unix_seconds,
    );
    let bytes = diagnostics.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_observability_event_json(
    provider_diagnostics_json: *const c_char,
    attempt_plan_json: *const c_char,
    loop_plan_json: *const c_char,
    observed_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let provider_diagnostics_json = c_char_to_string(provider_diagnostics_json).unwrap_or_default();
    let attempt_plan_json = c_char_to_string(attempt_plan_json).unwrap_or_default();
    let loop_plan_json = c_char_to_string(loop_plan_json).unwrap_or_default();
    let event = browser_data::browser_storage_sync_cloud_observability_event_json(
        &provider_diagnostics_json,
        &attempt_plan_json,
        &loop_plan_json,
        observed_at_unix_seconds,
    );
    let bytes = event.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_cycle_attempt_plan_json(
    cycle_round_json: *const c_char,
    retry_attempt: u32,
    observed_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let cycle_round_json = c_char_to_string(cycle_round_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_cycle_attempt_plan_json(
        &cycle_round_json,
        retry_attempt,
        observed_at_unix_seconds,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_membership_attempt_plan_json(
    membership_round_json: *const c_char,
    retry_attempt: u32,
    observed_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let membership_round_json = c_char_to_string(membership_round_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_membership_attempt_plan_json(
        &membership_round_json,
        retry_attempt,
        observed_at_unix_seconds,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_membership_loop_plan_json(
    membership_rounds_json: *const c_char,
    retry_attempt: u32,
    observed_at_unix_seconds: i64,
    max_attempts: u32,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let membership_rounds_json = c_char_to_string(membership_rounds_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_membership_loop_plan_json(
        &membership_rounds_json,
        retry_attempt,
        observed_at_unix_seconds,
        max_attempts,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_cycle_loop_plan_json(
    cycle_rounds_json: *const c_char,
    retry_attempt: u32,
    observed_at_unix_seconds: i64,
    max_attempts: u32,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let cycle_rounds_json = c_char_to_string(cycle_rounds_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_cloud_cycle_loop_plan_json(
        &cycle_rounds_json,
        retry_attempt,
        observed_at_unix_seconds,
        max_attempts,
    );
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cloud_local_folder_cycle_round_json(
    root_dir: *const c_char,
    push_request_json: *const c_char,
    cursors_json: *const c_char,
    device_id: *const c_char,
    generated_at_unix_seconds: i64,
    max_log_objects_per_collection: u32,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let root_dir = c_char_to_string(root_dir).unwrap_or_default();
    let push_request_json = c_char_to_string(push_request_json).unwrap_or_default();
    let cursors_json = c_char_to_string(cursors_json).unwrap_or_default();
    let device_id = c_char_to_string(device_id).unwrap_or_default();
    let round = browser_data::browser_storage_sync_cloud_local_folder_cycle_round_json(
        &root_dir,
        &push_request_json,
        &cursors_json,
        &device_id,
        generated_at_unix_seconds,
        max_log_objects_per_collection,
    );
    let bytes = round.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cycle_request_plan_json(
    base_url: *const c_char,
    access_token: *const c_char,
    push_request_json: *const c_char,
    cursors_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let base_url = c_char_to_string(base_url).unwrap_or_default();
    let access_token = c_char_to_string(access_token).unwrap_or_default();
    let push_request_json = c_char_to_string(push_request_json).unwrap_or_default();
    let cursors_json = c_char_to_string(cursors_json).unwrap_or_default();
    let request = browser_data::browser_storage_sync_cycle_request_plan_json(
        &base_url,
        &access_token,
        &push_request_json,
        &cursors_json,
    );
    let bytes = request.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_cycle_transport_result_json(
    status_code: i32,
    response_json: *const c_char,
    error_message: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let response_json = c_char_to_string(response_json).unwrap_or_default();
    let error_message = c_char_to_string(error_message).unwrap_or_default();
    let result = browser_data::browser_storage_sync_cycle_transport_result_json(
        status_code,
        &response_json,
        &error_message,
    );
    let bytes = result.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_status_policy_json(
    classification: *const c_char,
    should_retry: bool,
    client_action: *const c_char,
    retry_attempt: u32,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let classification = c_char_to_string(classification).unwrap_or_default();
    let client_action = c_char_to_string(client_action).unwrap_or_default();
    let policy = browser_data::browser_storage_sync_status_policy_json(
        &classification,
        should_retry,
        &client_action,
        retry_attempt,
    );
    let bytes = policy.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_schedule_state_json(
    status_policy_json: *const c_char,
    observed_at_unix_seconds: i64,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let status_policy_json = c_char_to_string(status_policy_json).unwrap_or_default();
    let state = browser_data::browser_storage_sync_schedule_state_json(
        &status_policy_json,
        observed_at_unix_seconds,
    );
    let bytes = state.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_scheduler_status_json(
    scheduler_snapshot_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let scheduler_snapshot_json = c_char_to_string(scheduler_snapshot_json).unwrap_or_default();
    let status = browser_data::browser_storage_sync_scheduler_status_json(&scheduler_snapshot_json);
    let bytes = status.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_ack_plan_json(
    ack_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let ack_json = c_char_to_string(ack_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_ack_plan_json(&ack_json);
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_remote_apply_plan_json(
    batch_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let batch_json = c_char_to_string(batch_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_remote_apply_plan_json(&batch_json);
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_remote_apply_result_json(
    plan_json: *const c_char,
    result_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let plan_json = c_char_to_string(plan_json).unwrap_or_default();
    let result_json = c_char_to_string(result_json).unwrap_or_default();
    let summary =
        browser_data::browser_storage_sync_remote_apply_result_json(&plan_json, &result_json);
    let bytes = summary.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_server_merge_plan_json(
    push_request_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let push_request_json = c_char_to_string(push_request_json).unwrap_or_default();
    let plan = browser_data::browser_storage_sync_server_merge_plan_json(&push_request_json);
    let bytes = plan.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_storage_sync_server_merge_result_json(
    plan_json: *const c_char,
    result_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let plan_json = c_char_to_string(plan_json).unwrap_or_default();
    let result_json = c_char_to_string(result_json).unwrap_or_default();
    let summary =
        browser_data::browser_storage_sync_server_merge_result_json(&plan_json, &result_json);
    let bytes = summary.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_history_max_items() -> u32 {
    browser_data::MAX_HISTORY_ITEMS as u32
}

#[no_mangle]
pub extern "C" fn actro_core_browser_bookmark_max_items() -> u32 {
    browser_data::MAX_BOOKMARK_ITEMS as u32
}

#[no_mangle]
pub extern "C" fn actro_core_browser_session_max_tabs() -> u32 {
    browser_data::MAX_SESSION_TABS as u32
}

#[no_mangle]
pub extern "C" fn actro_core_browser_data_max_title_length() -> u32 {
    browser_data::MAX_TITLE_LENGTH as u32
}

#[no_mangle]
pub extern "C" fn actro_core_browser_home_address() -> *const c_char {
    c_str_ptr(browser_data::HOME_ADDRESS)
}

#[no_mangle]
pub extern "C" fn actro_core_browser_history_address() -> *const c_char {
    c_str_ptr(browser_data::HISTORY_ADDRESS)
}

#[no_mangle]
pub extern "C" fn actro_core_browser_bookmarks_address() -> *const c_char {
    c_str_ptr(browser_data::BOOKMARKS_ADDRESS)
}

#[no_mangle]
pub extern "C" fn actro_core_browser_history_snapshot_normalize_json(
    snapshot_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let snapshot_json = c_char_to_string(snapshot_json).unwrap_or_default();
    let normalized = browser_data::normalize_history_snapshot_json(&snapshot_json);
    let bytes = normalized.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_bookmark_snapshot_normalize_json(
    snapshot_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let snapshot_json = c_char_to_string(snapshot_json).unwrap_or_default();
    let normalized = browser_data::normalize_bookmark_snapshot_json(&snapshot_json);
    let bytes = normalized.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_session_snapshot_normalize_json(
    snapshot_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let snapshot_json = c_char_to_string(snapshot_json).unwrap_or_default();
    let normalized = browser_data::normalize_session_snapshot_json(&snapshot_json);
    let bytes = normalized.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

#[no_mangle]
pub extern "C" fn actro_core_browser_session_home_snapshot_json(
    buffer: *mut c_char,
    buffer_len: usize,
) -> usize {
    let normalized = browser_data::home_session_snapshot_json();
    let bytes = normalized.as_bytes();
    write_c_string_buffer(bytes, buffer, buffer_len);
    bytes.len()
}

/// Returns a stable, NUL-terminated view of a `'static` string constant for
/// the C ABI, interning a `CString` copy on first use.
///
/// The returned pointer stays valid for the rest of the process because of
/// two invariants:
/// - Cache entries are only ever inserted, never removed or overwritten, so
///   every interned `CString` lives until process exit.
/// - A `CString` owns its bytes in a separate heap allocation; when the
///   `HashMap` grows and moves its buckets, only the `CString` handle is
///   relocated, not the buffer the returned pointer refers to.
///
/// Together these make it safe to return the pointer after the mutex guard
/// is dropped. The key set is bounded: callers only pass `'static` constants
/// (product metadata, auth environments/URLs, search engine fields, browser
/// addresses), so the cache cannot grow unboundedly.
fn c_str_ptr(value: &'static str) -> *const c_char {
    static CACHE: OnceLock<Mutex<HashMap<&'static str, CString>>> = OnceLock::new();

    let mut cache = CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    cache
        .entry(value)
        .or_insert_with(|| {
            CString::new(value).unwrap_or_else(|_| {
                // Inputs are compile-time constants; an interior NUL means a
                // broken constant, not bad runtime data. Fail loudly in debug
                // builds and degrade to an empty string in release builds.
                debug_assert!(
                    false,
                    "static C API string contains interior NUL: {value:?}"
                );
                CString::default()
            })
        })
        .as_ptr()
}

fn c_char_to_string(pointer: *const c_char) -> Option<String> {
    if pointer.is_null() {
        return None;
    }

    unsafe { CStr::from_ptr(pointer).to_str().ok().map(ToOwned::to_owned) }
}

fn write_c_string_buffer(bytes: &[u8], buffer: *mut c_char, buffer_len: usize) {
    if buffer.is_null() || buffer_len == 0 {
        return;
    }

    let copy_len = bytes.len().min(buffer_len.saturating_sub(1));
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast::<u8>(), copy_len);
        *buffer.add(copy_len) = 0;
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::{CStr, CString};

    use super::*;

    #[test]
    fn c_api_exports_stable_metadata() {
        unsafe {
            assert_eq!(
                CStr::from_ptr(actro_core_product_name()).to_str(),
                Ok("Actro")
            );
            assert_eq!(
                CStr::from_ptr(actro_core_supported_contract_name()).to_str(),
                Ok("ask")
            );
        }

        assert_eq!(actro_core_c_api_version(), 1);
        assert_eq!(actro_core_supported_contract_version(), 1);
        assert_eq!(actro_core_schema_version(), 1);
        assert_eq!(actro_core_runtime_schema_version(), 4);
    }

    #[test]
    fn c_api_exports_auth_environment_contract() {
        unsafe {
            assert_eq!(
                CStr::from_ptr(actro_core_auth_environment_dev()).to_str(),
                Ok("dev")
            );
            assert_eq!(
                CStr::from_ptr(actro_core_auth_environment_beta()).to_str(),
                Ok("beta")
            );
            assert_eq!(
                CStr::from_ptr(actro_core_auth_environment_prod()).to_str(),
                Ok("prod")
            );
            assert_eq!(
                CStr::from_ptr(actro_core_auth_account_base_url(c"dev".as_ptr())).to_str(),
                Ok("http://account-actro.develenv.com")
            );
            assert_eq!(
                CStr::from_ptr(actro_core_auth_account_base_url(c"beta".as_ptr())).to_str(),
                Ok("https://account-actro.betaenv.com")
            );
            assert_eq!(
                CStr::from_ptr(actro_core_auth_account_base_url(c"prod".as_ptr())).to_str(),
                Ok("https://account.actro.chat")
            );
            assert_eq!(
                CStr::from_ptr(actro_core_auth_account_base_url(c"unknown".as_ptr())).to_str(),
                Ok("https://account.actro.chat")
            );
        }
    }

    #[test]
    fn c_api_exports_browser_search_contract() {
        unsafe {
            assert_eq!(
                CStr::from_ptr(actro_core_browser_search_engine_default_id()).to_str(),
                Ok("google")
            );
            assert_eq!(actro_core_browser_search_engine_count(), 3);
            assert_eq!(
                CStr::from_ptr(actro_core_browser_search_engine_id(2)).to_str(),
                Ok("duckduckgo")
            );
            assert_eq!(
                CStr::from_ptr(actro_core_browser_search_engine_display_name(1)).to_str(),
                Ok("Bing")
            );
            assert_eq!(
                CStr::from_ptr(actro_core_browser_search_engine_url_prefix(0)).to_str(),
                Ok("https://www.google.com/search?q=")
            );
            assert_eq!(
                CStr::from_ptr(actro_core_browser_search_engine_normalize_id(
                    c" DuckDuckGo ".as_ptr()
                ))
                .to_str(),
                Ok("duckduckgo")
            );

            let mut buffer = [0 as c_char; 128];
            let required = actro_core_browser_search_url(
                c"actro browser".as_ptr(),
                c"duckduckgo".as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len(),
            );
            assert_eq!(required, "https://duckduckgo.com/?q=actro%20browser".len());
            assert_eq!(
                CStr::from_ptr(buffer.as_ptr()).to_str(),
                Ok("https://duckduckgo.com/?q=actro%20browser")
            );

            let mut small_buffer = [0 as c_char; 8];
            let required = actro_core_browser_search_url(
                c"a&b=c".as_ptr(),
                c"google".as_ptr(),
                small_buffer.as_mut_ptr(),
                small_buffer.len(),
            );
            assert_eq!(required, "https://www.google.com/search?q=a%26b%3Dc".len());
            assert_eq!(
                CStr::from_ptr(small_buffer.as_ptr()).to_str(),
                Ok("https:/")
            );
        }
    }

    #[test]
    fn c_api_exports_every_search_engine_field() {
        // Iterates the whole engine table so a newly added engine is covered
        // automatically instead of relying on hand-picked indices.
        assert_eq!(
            actro_core_browser_search_engine_count() as usize,
            browser::SEARCH_ENGINES.len()
        );
        for (index, engine) in browser::SEARCH_ENGINES.iter().enumerate() {
            let index = index as u32;
            unsafe {
                let id = CStr::from_ptr(actro_core_browser_search_engine_id(index))
                    .to_str()
                    .unwrap();
                let display_name =
                    CStr::from_ptr(actro_core_browser_search_engine_display_name(index))
                        .to_str()
                        .unwrap();
                let url_prefix = CStr::from_ptr(actro_core_browser_search_engine_url_prefix(index))
                    .to_str()
                    .unwrap();

                assert_eq!(id, engine.id);
                assert_eq!(display_name, engine.display_name);
                assert_eq!(url_prefix, engine.search_url_prefix);
                assert!(!id.is_empty());
                assert!(!display_name.is_empty());
                assert!(!url_prefix.is_empty());
            }
        }
    }

    #[test]
    fn c_api_exports_browser_data_contract() {
        unsafe {
            assert_eq!(actro_core_browser_data_schema_version(), 1);
            assert_eq!(actro_core_browser_storage_schema_version(), 6);
            assert_eq!(
                CStr::from_ptr(actro_core_browser_storage_database_name()).to_str(),
                Ok("actro_browser.db")
            );
            assert_eq!(actro_core_browser_history_max_items(), 500);
            assert_eq!(actro_core_browser_bookmark_max_items(), 500);
            assert_eq!(actro_core_browser_session_max_tabs(), 50);
            assert_eq!(actro_core_browser_data_max_title_length(), 160);
            assert_eq!(
                CStr::from_ptr(actro_core_browser_home_address()).to_str(),
                Ok("actro://home")
            );
            assert_eq!(
                CStr::from_ptr(actro_core_browser_history_address()).to_str(),
                Ok("actro://history")
            );
            assert_eq!(
                CStr::from_ptr(actro_core_browser_bookmarks_address()).to_str(),
                Ok("actro://bookmarks")
            );

            let input = c"{\"version\":99,\"items\":[{\"title\":\"\",\"address\":\"https://example.com/docs\",\"visitedAtUnixSeconds\":-1,\"visitCount\":0}]}";
            let mut buffer = [0 as c_char; 512];
            let required = actro_core_browser_history_snapshot_normalize_json(
                input.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"version\":1"));
            assert!(output.contains("\"title\":\"example.com\""));
            assert!(output.contains("\"visitedAtUnixSeconds\":0"));
            assert!(output.contains("\"visitCount\":1"));

            let input = c"{\"version\":99,\"items\":[{\"title\":\"Future\",\"address\":\"https://future.example\",\"createdAtUnixSeconds\":999999999999}]}";
            let required = actro_core_browser_bookmark_snapshot_normalize_json(
                input.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"createdAtUnixSeconds\":253402300799"));

            let input = c"{\"version\":99,\"activeTabIndex\":9,\"tabs\":[{\"title\":\"\",\"address\":\"ACTRO://HISTORY\"}]}";
            let required = actro_core_browser_session_snapshot_normalize_json(
                input.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"activeTabIndex\":0"));
            assert!(output.contains("\"title\":\"New Tab\""));
            assert!(output.contains("\"address\":\"actro://history\""));

            let required =
                actro_core_browser_session_home_snapshot_json(buffer.as_mut_ptr(), buffer.len());
            assert!(required > 0);
            let output = CStr::from_ptr(buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"address\":\"actro://home\""));

            let mut storage_buffer = [0 as c_char; 32768];
            let required = actro_core_browser_storage_schema_statements_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("browser_history_pages"));
            assert!(output.contains("browser_session_tabs"));

            let required = actro_core_browser_storage_schema_migration_plan_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            assert!(required < storage_buffer.len());
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"schemaVersion\":6"));
            assert!(output.contains("\"addColumns\""));
            assert!(output.contains("\"idempotentStatements\""));
            assert!(output.contains("browser_sync_cursors"));

            let required = actro_core_browser_storage_table_names_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("browser_bookmarks"));

            let required = actro_core_browser_storage_repository_sql_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("upsertHistoryPage"));
            assert!(output.contains("selectSession"));
            assert!(output.contains("browser_sessions"));

            let required = actro_core_browser_storage_sync_sql_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            assert!(required < storage_buffer.len());
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("selectDirtyBookmarks"));
            assert!(output.contains("browser_sync_cursors"));
            assert!(output.contains("applyRemoteBookmark"));

            let required = actro_core_browser_storage_data_model_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"snapshotModels\""));
            assert!(output.contains("\"recordType\":\"historyPage\""));
            assert!(output.contains("\"recordType\":\"sessionTab\""));
            assert!(output.contains("\"localSyncCursorTable\":\"browser_sync_cursors\""));

            let required = actro_core_browser_storage_sync_conflict_policy_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"policy\":\"skipLocalDirtyOrNewerRevision\""));
            assert!(output.contains("\"reason\":\"localDirtyOrNewerRevision\""));
            assert!(output.contains("\"appliesToRecordTypes\""));

            let required = actro_core_browser_storage_sync_merge_policy_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"mergePolicy\":\"bookmarkUpsertByStableId\""));
            assert!(output.contains("\"collection\":\"sessionTabs\""));
            assert!(output.contains("\"remoteApplyConflictPolicy\""));

            let required = actro_core_browser_storage_address_canonicalization_policy_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"policy\":\"browserStorageAddressCanonicalization\""));
            assert!(output.contains("\"acceptedExternalSchemes\""));

            let required = actro_core_browser_storage_canonicalize_addresses_json(
                c"{\"addresses\":[\" HTTP://Example.COM:80/Path \",\"ftp://example.com/file\"]}"
                    .as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"canonicalAddress\":\"http://example.com/Path\""));
            assert!(output.contains("\"reason\":\"unsupportedScheme\""));

            let required = actro_core_browser_storage_bookmark_folder_tree_integrity_policy_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"policy\":\"browserStorageBookmarkFolderTreeIntegrity\""));
            assert!(output.contains("\"integrityRules\""));

            let required = actro_core_browser_storage_bookmark_folder_tree_integrity_plan_json(
                c"{\"folders\":[{\"id\":\"a\",\"parentId\":\"b\"},{\"id\":\"b\",\"parentId\":\"a\"}]}"
                    .as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"classification\":\"cycleDetected\""));
            assert!(output.contains("\"action\":\"rejectFolderMove\""));

            let required = actro_core_browser_storage_bookmark_restore_placement_policy_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"policy\":\"browserStorageBookmarkRestorePlacement\""));
            assert!(output.contains("\"placementRules\""));

            let required = actro_core_browser_storage_bookmark_restore_placement_plan_json(
                c"{\"folders\":[{\"id\":\"old\",\"deletedAtUnixSeconds\":1}],\"bookmarks\":[{\"id\":\"bookmark-a\",\"folderId\":\"old\",\"title\":\"A\",\"address\":\"https://example.com/a\"}]}"
                    .as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"classification\":\"reassigned\""));
            assert!(output.contains("\"placementAction\":\"movedToRootAfterFolderDeletion\""));

            let required = actro_core_browser_storage_bookmark_ownership_policy_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"policy\":\"browserStorageBookmarkOwnership\""));
            assert!(output.contains("\"ownershipRules\""));

            let required = actro_core_browser_storage_bookmark_ownership_plan_json(
                c"{\"bookmarks\":[{\"id\":\"user-bookmark\",\"title\":\"User\",\"address\":\"https://example.com/a\",\"ownerKind\":\"user\",\"createdAtUnixSeconds\":1,\"updatedAtUnixSeconds\":2},{\"id\":\"tab-bookmark\",\"title\":\"Tab\",\"address\":\"https://example.com/a\",\"ownerKind\":\"tab\",\"ownerId\":\"tab-a\",\"createdAtUnixSeconds\":1,\"updatedAtUnixSeconds\":3}]}"
                    .as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"classification\":\"deduplicated\""));
            assert!(output.contains("\"ownershipAction\":\"retainUserOwned\""));

            let required = actro_core_browser_storage_history_visit_aggregation_policy_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"policy\":\"browserStorageHistoryVisitAggregation\""));
            assert!(output.contains("\"aggregationRules\""));

            let required = actro_core_browser_storage_history_visit_aggregation_plan_json(
                c"{\"visits\":[{\"id\":\"visit-1\",\"address\":\"HTTP://Example.COM:80/docs\",\"title\":\"One\",\"visitedAtUnixSeconds\":100},{\"id\":\"visit-2\",\"address\":\"http://example.com/docs\",\"title\":\"Two\",\"visitedAtUnixSeconds\":200}]}"
                    .as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"classification\":\"aggregated\""));
            assert!(output.contains("\"visitCount\":2"));

            let required = actro_core_browser_storage_session_tab_ordering_policy_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"policy\":\"browserStorageSessionTabOrdering\""));
            assert!(output.contains("\"orderingRules\""));

            let required = actro_core_browser_storage_session_tab_ordering_plan_json(
                c"{\"sessionTabs\":[{\"sessionId\":\"session-a\",\"tabId\":\"tab-b\",\"tabIndex\":2,\"title\":\"B\",\"address\":\"https://example.com/b\"},{\"sessionId\":\"session-a\",\"tabId\":\"tab-a\",\"tabIndex\":1,\"title\":\"A\",\"address\":\"https://example.com/a\"}]}"
                    .as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"classification\":\"ordered\""));
            assert!(output.contains("\"orderedTabIds\":[\"tab-a\",\"tab-b\"]"));

            let required = actro_core_browser_storage_session_tab_id_plan_json(
                c"[{\"id\":\"\",\"index\":0,\"address\":\"actro://home\"},{\"id\":\"tab-a\",\"index\":1,\"address\":\"https://example.com/a\"}]"
                    .as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"policy\":\"browserStorageSessionTabId\""));
            assert!(
                output.contains("\"tabIds\":[\"fa3d824d-efbc-593f-b327-a463b5e0973c\",\"tab-a\"]")
            );

            let required = actro_core_browser_storage_session_active_tab_consistency_policy_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"policy\":\"browserStorageSessionActiveTabConsistency\""));
            assert!(output.contains("\"consistencyRules\""));

            let required = actro_core_browser_storage_session_active_tab_consistency_plan_json(
                c"{\"sessions\":[{\"id\":\"session-a\",\"activeTabIndex\":9}],\"sessionTabs\":[{\"sessionId\":\"session-a\",\"tabId\":\"tab-a\",\"tabIndex\":0,\"title\":\"A\",\"address\":\"https://example.com/a\"}]}"
                    .as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"classification\":\"adjusted\""));
            assert!(output.contains("\"activeTabId\":\"tab-a\""));

            let required = actro_core_browser_storage_retention_pruning_policy_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"policy\":\"browserStorageRetentionPruning\""));
            assert!(output.contains("\"maxHistoryPageCount\""));

            let required = actro_core_browser_storage_retention_pruning_plan_json(
                c"{\"maxHistoryPages\":1,\"maxSessionTabsPerSession\":1,\"historyPages\":[{\"address\":\"https://example.com/a\",\"lastVisitedAtUnixSeconds\":1},{\"address\":\"https://example.com/b\",\"lastVisitedAtUnixSeconds\":2}],\"sessionTabs\":[{\"sessionId\":\"session-a\",\"tabId\":\"tab-a\",\"tabIndex\":0,\"title\":\"A\",\"address\":\"https://example.com/a\"},{\"sessionId\":\"session-a\",\"tabId\":\"tab-b\",\"tabIndex\":1,\"title\":\"B\",\"address\":\"https://example.com/b\"}]}"
                    .as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"classification\":\"pruneHistoryAndSessions\""));
            assert!(output.contains("\"reason\":\"historyPageLimitExceeded\""));
            assert!(output.contains("\"reason\":\"sessionTabLimitExceeded\""));

            let required = actro_core_browser_storage_sync_batch_empty_json(
                c"".as_ptr(),
                c"".as_ptr(),
                -1,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"storageSchemaVersion\":6"));

            let required = actro_core_browser_storage_sync_batch_normalize_json(
                c"{\"version\":99,\"storageSchemaVersion\":6,\"scope\":\"profile\",\"deviceId\":\"device-a\",\"generatedAtUnixSeconds\":1,\"records\":[]}".as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"scope\":\"profile\""));
            assert!(output.contains("\"records\":[]"));

            let required = actro_core_browser_storage_sync_batch_from_dirty_rows_json(
                c"profile".as_ptr(),
                c"device-a".as_ptr(),
                2,
                c"[{\"record_type\":\"historyPage\",\"record_id\":\"https://example.com\",\"sync_revision\":1,\"sync_state\":\"local\",\"origin_device_id\":\"\",\"updated_at\":1,\"address\":\"https://example.com\",\"title\":\"\",\"visit_count\":1,\"last_visited_at\":1,\"created_at\":1}]".as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"recordType\":\"historyPage\""));
            assert!(output.contains("\"title\":\"example.com\""));

            let required = actro_core_browser_storage_sync_push_request_from_dirty_rows_json(
                c"profile".as_ptr(),
                c"device-a".as_ptr(),
                2,
                c"[{\"record_type\":\"historyPage\",\"record_id\":\"https://example.com\",\"sync_revision\":1,\"sync_state\":\"local\",\"origin_device_id\":\"\",\"updated_at\":1,\"address\":\"https://example.com\",\"title\":\"\",\"visit_count\":1,\"last_visited_at\":1,\"created_at\":1}]".as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"requestType\":\"push\""));
            assert!(output.contains("\"pushBatch\""));

            let required = actro_core_browser_storage_sync_ack_batch_normalize_json(
                c"{\"version\":99,\"storageSchemaVersion\":6,\"scope\":\"profile\",\"deviceId\":\"device-a\",\"syncedAtUnixSeconds\":1,\"records\":[],\"cursors\":[{\"collection\":\"historyPages\",\"cursor\":\"cursor-a\"}]}".as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"storageSchemaVersion\":6"));
            assert!(output.contains("\"lastSyncedAtUnixSeconds\":1"));

            let required = actro_core_browser_storage_sync_cursors_normalize_json(
                c"[{\"collection\":\"historyPages\",\"cursor\":\"cursor-a\"},{\"collection\":\"bad\",\"cursor\":\"ignored\"}]".as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"collection\":\"historyPages\""));
            assert!(!output.contains("ignored"));

            let required = actro_core_browser_storage_sync_push_request_json(
                c"{\"version\":1,\"storageSchemaVersion\":6,\"scope\":\"profile\",\"deviceId\":\"device-a\",\"generatedAtUnixSeconds\":1,\"records\":[]}".as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"requestType\":\"push\""));
            assert!(output.contains("\"pushBatch\""));

            let required = actro_core_browser_storage_sync_pull_request_json(
                c"profile".as_ptr(),
                c"device-a".as_ptr(),
                2,
                c"[{\"collection\":\"historyPages\",\"cursor\":\"cursor-a\"}]".as_ptr(),
                0,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"requestType\":\"pull\""));
            assert!(output.contains("\"maxRecordsPerCollection\":200"));

            let required = actro_core_browser_storage_sync_cycle_response_json(
                c"{\"version\":1,\"storageSchemaVersion\":6,\"scope\":\"profile\",\"deviceId\":\"device-a\",\"generatedAtUnixSeconds\":2,\"records\":[]}".as_ptr(),
                c"{\"version\":1,\"storageSchemaVersion\":6,\"scope\":\"profile\",\"deviceId\":\"device-a\",\"syncedAtUnixSeconds\":1,\"records\":[],\"cursors\":[]}".as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"responseType\":\"syncCycle\""));
            assert!(output.contains("\"remoteBatch\""));
            assert!(output.contains("\"ackBatch\""));

            let required = actro_core_browser_storage_sync_cycle_response_normalize_json(
                c"{\"version\":99,\"storageSchemaVersion\":6,\"responseType\":\"custom\",\"scope\":\"profile\",\"deviceId\":\"device-a\",\"generatedAtUnixSeconds\":2,\"remoteBatch\":{\"version\":1,\"storageSchemaVersion\":6,\"scope\":\"profile\",\"deviceId\":\"device-a\",\"generatedAtUnixSeconds\":2,\"records\":[]},\"ackBatch\":{\"version\":1,\"storageSchemaVersion\":6,\"scope\":\"profile\",\"deviceId\":\"device-a\",\"syncedAtUnixSeconds\":1,\"records\":[],\"cursors\":[]}}".as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"responseType\":\"syncCycle\""));

            let required = actro_core_browser_storage_sync_client_cycle_plan_json(
                c"{\"version\":1,\"storageSchemaVersion\":6,\"responseType\":\"syncCycle\",\"scope\":\"profile\",\"deviceId\":\"device-a\",\"generatedAtUnixSeconds\":2,\"remoteBatch\":{\"version\":1,\"storageSchemaVersion\":6,\"scope\":\"profile\",\"deviceId\":\"device-a\",\"generatedAtUnixSeconds\":2,\"records\":[]},\"ackBatch\":{\"version\":1,\"storageSchemaVersion\":6,\"scope\":\"profile\",\"deviceId\":\"device-a\",\"syncedAtUnixSeconds\":1,\"records\":[],\"cursors\":[]}}".as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"action\":\"noop\""));
            assert!(output.contains("\"remoteApplyPlan\""));

            let required = actro_core_browser_storage_sync_client_ack_batch_after_remote_apply_json(
                c"{\"version\":1,\"storageSchemaVersion\":6,\"responseType\":\"syncCycle\",\"scope\":\"profile\",\"deviceId\":\"device-a\",\"generatedAtUnixSeconds\":2,\"remoteBatch\":{\"version\":1,\"storageSchemaVersion\":6,\"scope\":\"profile\",\"deviceId\":\"device-a\",\"generatedAtUnixSeconds\":2,\"records\":[]},\"ackBatch\":{\"version\":1,\"storageSchemaVersion\":6,\"scope\":\"profile\",\"deviceId\":\"device-a\",\"syncedAtUnixSeconds\":1,\"records\":[],\"cursors\":[{\"collection\":\"historyPages\",\"cursor\":\"cursor-a\"}]}}".as_ptr(),
                c"null".as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"cursors\":["));
            assert!(output.contains("\"cursor\":\"cursor-a\""));

            let required = actro_core_browser_storage_sync_client_cycle_apply_result_json(
                c"{\"version\":1,\"storageSchemaVersion\":6,\"responseType\":\"syncCycle\",\"scope\":\"profile\",\"deviceId\":\"device-a\",\"generatedAtUnixSeconds\":2,\"remoteBatch\":{\"version\":1,\"storageSchemaVersion\":6,\"scope\":\"profile\",\"deviceId\":\"device-a\",\"generatedAtUnixSeconds\":2,\"records\":[]},\"ackBatch\":{\"version\":1,\"storageSchemaVersion\":6,\"scope\":\"profile\",\"deviceId\":\"device-a\",\"syncedAtUnixSeconds\":1,\"records\":[],\"cursors\":[{\"collection\":\"historyPages\",\"cursor\":\"cursor-a\"}]}}".as_ptr(),
                c"null".as_ptr(),
                true,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"ackApplied\":true"));
            assert!(output.contains("\"cursorAckDeferred\":false"));
            assert!(output.contains("\"cyclePlan\""));

            let required = actro_core_browser_storage_sync_cloud_cycle_client_apply_result_json(
                c"{\"clientPlan\":{\"version\":1,\"storageSchemaVersion\":6,\"cloudDataModelVersion\":1,\"planType\":\"cycleClientPlan\",\"scope\":\"profile\",\"deviceId\":\"device-a\",\"generatedAtUnixSeconds\":2,\"action\":\"applyLocalAck\",\"steps\":[{\"stepType\":\"applyLocalAck\",\"operationCount\":1,\"required\":true,\"failureAction\":\"retry local ack\"}]}}".as_ptr(),
                c"[{\"stepType\":\"applyLocalAck\",\"success\":false,\"error\":null}]".as_ptr(),
                c"null".as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"failedStep\":\"applyLocalAck\""));
            assert!(output.contains("\"missing ackPlan\""));
            assert!(output.contains("\"applied\":false"));

            let required = actro_core_browser_storage_sync_transport_contract_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("/v1/browser-storage/sync/cycle"));
            assert!(output.contains("\"Authorization\""));

            let required = actro_core_browser_storage_sync_cloud_store_contract_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"rootPath\":\"Actro/BrowserSync\""));
            assert!(output.contains("\"operation\":\"write\""));
            assert!(output.contains("expectedRevision"));

            let required = actro_core_browser_storage_sync_cloud_provider_adapter_contract_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudProviderAdapterPlan\""));
            assert!(output.contains("coarse rounds"));

            let required = actro_core_browser_storage_sync_cloud_provider_adapter_plan_json(
                c"{\"providerKind\":\"localFolder\",\"rootPath\":\"Actro/BrowserSync\",\"revisionKind\":\"contentHash\",\"supportsPrefixList\":true,\"supportsPagedList\":true,\"supportsConditionalWrite\":true,\"supportsConditionalDelete\":true,\"supportsReadIfChanged\":true,\"supportsAtomicRename\":false,\"supportsAppFolder\":true,\"backgroundSync\":\"platformScheduled\",\"conflictResolution\":\"optimisticRevision\",\"maxObjectBytes\":8388608,\"maxPageSize\":1000}".as_ptr(),
                90,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"classification\":\"ready\""));
            assert!(output.contains("\"productionReady\":true"));

            let required = actro_core_browser_storage_sync_cloud_provider_connection_contract_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudProviderConnectionStatusPolicy\""));

            let required = actro_core_browser_storage_sync_cloud_provider_error_policy_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudProviderErrorClassification\""));
            assert!(output.contains("coarse native/provider rounds"));

            let required =
                actro_core_browser_storage_sync_cloud_provider_connection_status_policy_json(
                    c"{\"providerKind\":\"localFolder\",\"connectionState\":\"connected\",\"credentialState\":\"available\",\"membershipStatus\":\"active\",\"cloudStoreReady\":true}".as_ptr(),
                    0,
                    90,
                    storage_buffer.as_mut_ptr(),
                    storage_buffer.len(),
                );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"classification\":\"ready\""));
            assert!(output.contains("\"shouldAttemptSync\":true"));

            let required = actro_core_normalize_browser_sync_provider_settings_json(
                c"{\"providerKind\":\"localFolder\",\"localFolderRootPath\":\"/Users/leo/Sync\",\"scope\":\"profile\",\"deviceId\":\"device-a\",\"savedAtUnixSeconds\":1901}".as_ptr(),
                true,
                90,
                c"fresh-id".as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"configured\":true"));
            assert!(output.contains("\"deviceId\":\"device-a\""));

            let required =
                actro_core_browser_storage_sync_cloud_provider_error_classification_json(
                    c"{\"providerKind\":\"googleDrive\",\"errors\":[{\"operationIndex\":1,\"operationType\":\"writeManifest\",\"providerStatusCode\":412,\"message\":\"etag mismatch\"}]}".as_ptr(),
                    0,
                    90,
                    storage_buffer.as_mut_ptr(),
                    storage_buffer.len(),
                );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"classification\":\"cloudStateRereadRequired\""));
            assert!(output.contains("\"canonicalErrorKind\":\"conflict\""));
            let provider_error_classification_c =
                std::ffi::CString::new(output.to_string()).unwrap();

            let required = actro_core_browser_storage_sync_cloud_provider_diagnostics_json(
                c"".as_ptr(),
                c"".as_ptr(),
                provider_error_classification_c.as_ptr(),
                c"".as_ptr(),
                c"".as_ptr(),
                91,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"classification\":\"cloudConflict\""));
            assert!(output.contains("\"providerErrorClassification\":\"cloudStateRereadRequired\""));
            assert!(output.contains("\"pendingCloudReread\":true"));

            let required =
                actro_core_browser_storage_sync_cloud_store_capability_preflight_json(
                    c"{\"providerKind\":\"memory\",\"rootPath\":\"Actro/BrowserSync\",\"revisionKind\":\"monotonicCounter\",\"supportsPrefixList\":true,\"supportsPagedList\":true,\"supportsConditionalWrite\":true,\"supportsConditionalDelete\":true,\"supportsReadIfChanged\":true,\"supportsAtomicRename\":false,\"supportsAppFolder\":true,\"backgroundSync\":\"testControlled\",\"conflictResolution\":\"optimisticRevision\",\"maxObjectBytes\":8388608,\"maxPageSize\":1000}".as_ptr(),
                    4,
                    storage_buffer.as_mut_ptr(),
                    storage_buffer.len(),
                );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudStoreCapabilityPreflight\""));
            assert!(output.contains("\"classification\":\"ready\""));

            let required = actro_core_browser_storage_sync_cloud_local_folder_capabilities_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"providerKind\":\"localFolder\""));
            assert!(output.contains("\"revisionKind\":\"contentHash\""));

            let required =
                actro_core_browser_storage_sync_cloud_local_folder_capability_preflight_json(
                    4,
                    storage_buffer.as_mut_ptr(),
                    storage_buffer.len(),
                );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudStoreCapabilityPreflight\""));
            assert!(output.contains("\"providerKind\":\"localfolder\""));
            assert!(output.contains("\"classification\":\"ready\""));

            let local_folder_compatibility_root = std::env::temp_dir().join(format!(
                "actro-core-c-api-local-folder-compatibility-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&local_folder_compatibility_root);
            let local_folder_compatibility_root_c =
                CString::new(local_folder_compatibility_root.to_string_lossy().as_bytes()).unwrap();
            let required =
                actro_core_browser_storage_sync_cloud_local_folder_compatibility_preflight_json(
                    local_folder_compatibility_root_c.as_ptr(),
                    4,
                    storage_buffer.as_mut_ptr(),
                    storage_buffer.len(),
                );
            let _ = std::fs::remove_dir_all(&local_folder_compatibility_root);
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudCompatibilityPreflight\""));
            assert!(output.contains("\"classification\":\"compatible\""));
            assert!(output.contains("\"documentCount\":0"));

            let required = actro_core_browser_storage_sync_cloud_high_watermark_policy_json(
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"policyName\":\"browserStorageSyncCloudHighWatermark\""));

            let required = actro_core_browser_storage_sync_cloud_high_watermark_plan_json(
                c"{\"version\":1,\"storageSchemaVersion\":6,\"scope\":\"profile\",\"updatedAtUnixSeconds\":1,\"writerDeviceId\":\"device-a\",\"devices\":[],\"collections\":[],\"retention\":{\"retainLogSequences\":8,\"retainSnapshots\":2,\"compactWhenLogSequencesExceed\":8},\"compatibility\":{\"minStorageSchemaVersion\":6,\"maxStorageSchemaVersion\":6,\"minCloudDataModelVersion\":1,\"maxCloudDataModelVersion\":1,\"unsupportedClientAction\":\"upgradeClient\"}}".as_ptr(),
                c"[]".as_ptr(),
                c"[]".as_ptr(),
                c"[]".as_ptr(),
                c"device-a".as_ptr(),
                4,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudHighWatermarkRetention\""));
            assert!(output.contains("\"allCurrent\":true"));
            let high_watermark_plan = std::ffi::CString::new(output).unwrap();

            let required = actro_core_browser_storage_sync_cloud_cleanup_result_plan_json(
                high_watermark_plan.as_ptr(),
                c"[]".as_ptr(),
                4,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudCleanupResult\""));
            assert!(output.contains("\"clientAction\":\"nothingToCleanup\""));

            let required = actro_core_browser_storage_sync_cloud_compatibility_status_json(
                c"manifest".as_ptr(),
                c"{\"version\":1,\"storageSchemaVersion\":6,\"scope\":\"profile\",\"updatedAtUnixSeconds\":1,\"writerDeviceId\":\"device-a\",\"devices\":[],\"collections\":[],\"retention\":{\"retainLogSequences\":8,\"retainSnapshots\":2,\"compactWhenLogSequencesExceed\":8},\"compatibility\":{\"minStorageSchemaVersion\":6,\"maxStorageSchemaVersion\":6,\"minCloudDataModelVersion\":1,\"maxCloudDataModelVersion\":1,\"unsupportedClientAction\":\"upgradeClient\"}}".as_ptr(),
                2,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"classification\":\"compatible\""));
            assert!(output.contains("\"shouldWrite\":true"));

            let required = actro_core_browser_storage_sync_cloud_compatibility_preflight_json(
                c"[{\"path\":\"Actro/BrowserSync/manifest.json\",\"document\":{\"version\":1,\"storageSchemaVersion\":6,\"scope\":\"profile\",\"updatedAtUnixSeconds\":1,\"writerDeviceId\":\"device-a\",\"devices\":[],\"collections\":[],\"retention\":{\"retainLogSequences\":8,\"retainSnapshots\":2,\"compactWhenLogSequencesExceed\":8},\"compatibility\":{\"minStorageSchemaVersion\":6,\"maxStorageSchemaVersion\":6,\"minCloudDataModelVersion\":1,\"maxCloudDataModelVersion\":1,\"unsupportedClientAction\":\"upgradeClient\"}}}]".as_ptr(),
                3,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudCompatibilityPreflight\""));
            assert!(output.contains("\"shouldContinue\":true"));
            let compatibility_preflight_c = std::ffi::CString::new(output.to_string()).unwrap();

            let required = actro_core_browser_storage_sync_cloud_schema_rollout_plan_json(
                compatibility_preflight_c.as_ptr(),
                4,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudSchemaRolloutPlan\""));
            assert!(output.contains("\"classification\":\"ready\""));
            assert!(output.contains("\"compatibilityWindow\":\"currentOnly\""));
            let schema_rollout_c = std::ffi::CString::new(output.to_string()).unwrap();

            let required = actro_core_browser_storage_sync_cloud_schema_migration_playbook_json(
                schema_rollout_c.as_ptr(),
                5,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudSchemaMigrationPlaybook\""));
            assert!(output.contains("\"classification\":\"noMigrationRequired\""));
            assert!(output.contains("\"shouldAllowNormalSync\":true"));
            let schema_migration_playbook_c = std::ffi::CString::new(output.to_string()).unwrap();

            let required = actro_core_browser_storage_sync_cloud_recovery_snapshot_plan_json(
                schema_migration_playbook_c.as_ptr(),
                compatibility_preflight_c.as_ptr(),
                6,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudRecoverySnapshotPlan\""));
            assert!(output.contains("\"shouldSnapshot\":false"));
            let recovery_snapshot_plan_c = std::ffi::CString::new(output.to_string()).unwrap();

            let required = actro_core_browser_storage_sync_cloud_recovery_snapshot_result_plan_json(
                recovery_snapshot_plan_c.as_ptr(),
                c"[]".as_ptr(),
                7,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudRecoverySnapshotResult\""));
            assert!(output.contains("\"completed\":true"));

            let required =
                actro_core_browser_storage_sync_cloud_local_folder_recovery_snapshot_round_json(
                    c"".as_ptr(),
                    schema_migration_playbook_c.as_ptr(),
                    compatibility_preflight_c.as_ptr(),
                    8,
                    storage_buffer.as_mut_ptr(),
                    storage_buffer.len(),
                );
            assert_eq!(required, 4);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert_eq!(output, "null");

            let required = actro_core_browser_storage_sync_cloud_membership_plan_json(
                c"{\"scope\":\"profile\",\"actorDeviceId\":\"device-a\",\"memberDeviceId\":\"device-a\",\"membershipAction\":\"join\",\"generatedAtUnixSeconds\":3}".as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudMembership\""));
            assert!(output.contains("\"operationType\":\"writeManifest\""));
            let membership_plan_output = output.to_string();
            let membership_plan_c = std::ffi::CString::new(membership_plan_output).unwrap();

            let required = actro_core_browser_storage_sync_cloud_membership_result_plan_json(
                membership_plan_c.as_ptr(),
                c"[{\"operationIndex\":0,\"operationType\":\"writeDevice\",\"path\":\"Actro/BrowserSync/devices/device-a.json\",\"success\":true,\"providerRevision\":\"device-r1\"},{\"operationIndex\":1,\"operationType\":\"writeManifest\",\"path\":\"Actro/BrowserSync/manifest.json\",\"success\":true,\"providerRevision\":\"manifest-r1\"}]".as_ptr(),
                4,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudMembershipResult\""));
            assert!(output.contains("\"completed\":true"));

            let local_folder_membership_root = std::env::temp_dir().join(format!(
                "actro-core-c-api-local-folder-membership-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&local_folder_membership_root);
            let local_folder_membership_root_c =
                CString::new(local_folder_membership_root.to_string_lossy().as_bytes()).unwrap();
            let required =
                actro_core_browser_storage_sync_cloud_local_folder_membership_round_json(
                    local_folder_membership_root_c.as_ptr(),
                    c"{\"scope\":\"profile\",\"actorDeviceId\":\"device-a\",\"memberDeviceId\":\"device-a\",\"membershipAction\":\"join\",\"generatedAtUnixSeconds\":3}".as_ptr(),
                    storage_buffer.as_mut_ptr(),
                    storage_buffer.len(),
                );
            let _ = std::fs::remove_dir_all(&local_folder_membership_root);
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"membershipRound\""));
            assert!(output.contains("\"completed\":true"));
            let membership_round_c = std::ffi::CString::new(output.to_string()).unwrap();

            let required = actro_core_browser_storage_sync_cloud_membership_status_policy_json(
                membership_round_c.as_ptr(),
                0,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudMembershipStatusPolicy\""));
            assert!(output.contains("\"classification\":\"success\""));
            let membership_status_policy_c = std::ffi::CString::new(output.to_string()).unwrap();

            let required = actro_core_browser_storage_sync_cloud_membership_schedule_state_json(
                membership_status_policy_c.as_ptr(),
                10_000,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudMembershipScheduleState\""));
            assert!(output.contains("\"schedulerAction\":\"idle\""));

            let required = actro_core_browser_storage_sync_cloud_local_folder_compaction_round_json(
                c"".as_ptr(),
                c"device-a".as_ptr(),
                4,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert_eq!(required, 4);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert_eq!(output, "null");

            let required = actro_core_browser_storage_sync_cloud_local_folder_cleanup_round_json(
                c"".as_ptr(),
                c"[]".as_ptr(),
                c"device-a".as_ptr(),
                4,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert_eq!(required, 4);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert_eq!(output, "null");

            let required =
                actro_core_browser_storage_sync_cloud_local_folder_recovery_retention_round_json(
                    c"".as_ptr(),
                    2,
                    4,
                    storage_buffer.as_mut_ptr(),
                    storage_buffer.len(),
                );
            assert_eq!(required, 4);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert_eq!(output, "null");

            let required = actro_core_browser_storage_sync_cloud_pull_continuation_plan_json(
                c"{\"version\":1,\"storageSchemaVersion\":6,\"cloudDataModelVersion\":1,\"planType\":\"pullRead\",\"scope\":\"profile\",\"deviceId\":\"device-a\",\"generatedAtUnixSeconds\":1,\"manifestPath\":\"Actro/BrowserSync/manifest.json\",\"collections\":[{\"collection\":\"bookmarks\",\"localCursor\":\"\",\"remoteHighWatermarkCursor\":\"3\",\"latestLogSequence\":3,\"snapshotPath\":\"Actro/BrowserSync/snapshots/bookmarks.json\",\"snapshotRevision\":0,\"shouldReadSnapshot\":false,\"shouldListLogs\":true,\"logsPrefix\":\"Actro/BrowserSync/logs/bookmarks/\",\"maxLogObjects\":1}],\"operations\":[{\"operationIndex\":0,\"operationType\":\"listLogs\",\"prefix\":\"Actro/BrowserSync/logs/bookmarks/\",\"collection\":\"bookmarks\",\"maxObjects\":1,\"required\":true,\"resultContract\":\"BrowserStorageSyncCloudListResult\",\"conflictAction\":\"rereadManifestMergeAndRetry\"}],\"maxLogObjectsPerCollection\":1,\"resultPolicy\":\"test\",\"conflictPolicy\":\"rereadManifestMergeAndRetry\"}".as_ptr(),
                c"[{\"operationIndex\":0,\"operationType\":\"listLogs\",\"path\":\"Actro/BrowserSync/logs/bookmarks/\",\"collection\":\"bookmarks\",\"success\":true,\"exists\":true,\"objectCount\":1,\"nextCursor\":\"Actro/BrowserSync/logs/bookmarks/device-b-0000000000000001.json\"}]".as_ptr(),
                2,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"pullContinuation\""));
            assert!(output.contains(
                "\"pageCursor\":\"Actro/BrowserSync/logs/bookmarks/device-b-0000000000000001.json\""
            ));

            let required = actro_core_browser_storage_sync_cloud_cycle_status_policy_json(
                c"{\"scope\":\"profile\",\"deviceId\":\"device-a\",\"generatedAtUnixSeconds\":7,\"completed\":false,\"shouldRetry\":false,\"shouldReread\":false,\"shouldAckLocal\":false,\"shouldApplyRemote\":false,\"clientAction\":\"recoverProviderCredentials\",\"pushRound\":{\"resultPlan\":{\"issues\":[{\"reason\":\"unauthorized\",\"retryable\":false}]}}}".as_ptr(),
                0,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"classification\":\"providerAuthRequired\""));
            assert!(output.contains("\"providerAction\":\"reconnectProvider\""));
            let cycle_status_policy_c = std::ffi::CString::new(output.to_string()).unwrap();

            let required = actro_core_browser_storage_sync_cloud_cycle_schedule_state_json(
                cycle_status_policy_c.as_ptr(),
                10_000,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"schedulerAction\":\"waitForProviderCredentials\""));
            assert!(output.contains("\"waitingForProviderCredentials\":true"));
            let schedule_state_c = std::ffi::CString::new(output.to_string()).unwrap();

            let required = actro_core_browser_storage_sync_cloud_provider_diagnostics_json(
                c"{\"planType\":\"cloudStoreCapabilityPreflight\",\"providerKind\":\"localfolder\",\"rootPath\":\"Actro/BrowserSync\",\"classification\":\"ready\",\"shouldBlockSync\":false,\"clientAction\":\"continueCloudSync\",\"providerAction\":\"continueProviderSync\",\"userAction\":\"none\"}".as_ptr(),
                c"".as_ptr(),
                c"".as_ptr(),
                cycle_status_policy_c.as_ptr(),
                schedule_state_c.as_ptr(),
                10_001,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudProviderDiagnostics\""));
            assert!(output.contains("\"classification\":\"providerAuthRequired\""));
            assert!(output.contains("\"waitingForProviderCredentials\":true"));
            let provider_diagnostics_c = std::ffi::CString::new(output.to_string()).unwrap();

            let required = actro_core_browser_storage_sync_cloud_cycle_attempt_plan_json(
                c"{\"scope\":\"profile\",\"deviceId\":\"device-a\",\"generatedAtUnixSeconds\":7,\"completed\":false,\"shouldRetry\":false,\"shouldReread\":false,\"shouldAckLocal\":false,\"shouldApplyRemote\":false,\"clientAction\":\"recoverProviderCredentials\",\"pushRound\":{\"resultPlan\":{\"issues\":[{\"reason\":\"unauthorized\",\"retryable\":false}]}}}".as_ptr(),
                0,
                10_000,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudCycleAttemptPlan\""));
            assert!(output.contains("\"attemptAction\":\"reconnectProvider\""));
            assert!(output.contains("\"shouldStopUntilRecovery\":true"));

            let required = actro_core_browser_storage_sync_cloud_membership_attempt_plan_json(
                c"{\"scope\":\"profile\",\"actorDeviceId\":\"device-a\",\"memberDeviceId\":\"device-b\",\"membershipAction\":\"connect\",\"generatedAtUnixSeconds\":7,\"completed\":false,\"shouldRetry\":false,\"shouldReread\":false,\"clientAction\":\"recoverProviderCredentials\",\"resultPlan\":{\"issues\":[{\"reason\":\"unauthorized\",\"retryable\":false}]}}".as_ptr(),
                0,
                10_000,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudMembershipAttemptPlan\""));
            assert!(output.contains("\"attemptAction\":\"reconnectProvider\""));
            assert!(output.contains("\"shouldStopUntilRecovery\":true"));
            let cloud_membership_attempt_plan_c =
                std::ffi::CString::new(output.to_string()).unwrap();

            let required = actro_core_browser_storage_sync_cloud_membership_loop_plan_json(
                c"[{\"scope\":\"profile\",\"actorDeviceId\":\"device-a\",\"memberDeviceId\":\"device-b\",\"membershipAction\":\"connect\",\"generatedAtUnixSeconds\":7,\"completed\":false,\"shouldRetry\":false,\"shouldReread\":false,\"clientAction\":\"recoverProviderCredentials\",\"resultPlan\":{\"issues\":[{\"reason\":\"unauthorized\",\"retryable\":false}]}}]".as_ptr(),
                0,
                10_000,
                3,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudMembershipLoopPlan\""));
            assert!(output.contains("\"finalAction\":\"reconnectProvider\""));
            assert!(output.contains("\"shouldStopUntilRecovery\":true"));
            let cloud_membership_loop_plan_c = std::ffi::CString::new(output.to_string()).unwrap();

            let required = actro_core_browser_storage_sync_cloud_cycle_loop_plan_json(
                c"[{\"scope\":\"profile\",\"deviceId\":\"device-a\",\"generatedAtUnixSeconds\":7,\"completed\":false,\"shouldRetry\":false,\"shouldReread\":false,\"shouldAckLocal\":false,\"shouldApplyRemote\":false,\"clientAction\":\"recoverProviderCredentials\",\"pushRound\":{\"resultPlan\":{\"issues\":[{\"reason\":\"unauthorized\",\"retryable\":false}]}}}]".as_ptr(),
                0,
                10_000,
                3,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudCycleLoopPlan\""));
            assert!(output.contains("\"finalAction\":\"reconnectProvider\""));
            assert!(output.contains("\"shouldStopUntilRecovery\":true"));

            let required = actro_core_browser_storage_sync_cloud_observability_event_json(
                provider_diagnostics_c.as_ptr(),
                cloud_membership_attempt_plan_c.as_ptr(),
                cloud_membership_loop_plan_c.as_ptr(),
                10_002,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cloudObservabilityEvent\""));
            assert!(output.contains("\"eventName\":\"browserSyncCloudUserActionRequired\""));
            assert!(output.contains("\"redactedExportAllowed\":true"));

            let local_folder_root = std::env::temp_dir().join(format!(
                "actro-core-c-api-local-folder-cycle-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&local_folder_root);
            let local_folder_root_c =
                CString::new(local_folder_root.to_string_lossy().as_bytes()).unwrap();
            let required = actro_core_browser_storage_sync_cloud_local_folder_cycle_round_json(
                local_folder_root_c.as_ptr(),
                c"null".as_ptr(),
                c"[]".as_ptr(),
                c"device-a".as_ptr(),
                10_000,
                20,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            let _ = std::fs::remove_dir_all(&local_folder_root);
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"planType\":\"cycleRound\""));

            let required = actro_core_browser_storage_sync_cycle_request_plan_json(
                c"http://127.0.0.1:8787/".as_ptr(),
                c"token-a-123".as_ptr(),
                c"{\"version\":1,\"storageSchemaVersion\":6,\"requestType\":\"push\",\"scope\":\"profile\",\"deviceId\":\"device-a\",\"generatedAtUnixSeconds\":6,\"pushBatch\":{\"version\":1,\"storageSchemaVersion\":6,\"scope\":\"profile\",\"deviceId\":\"device-a\",\"generatedAtUnixSeconds\":6,\"records\":[]}}".as_ptr(),
                c"[{\"collection\":\"historyPages\",\"cursor\":\"7\"}]".as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"method\":\"POST\""));
            assert!(output.contains("/v1/browser-storage/sync/cycle"));
            assert!(output.contains("\"value\":\"Bearer token-a-123\""));
            assert!(output.contains("\"bodyJson\""));

            let required = actro_core_browser_storage_sync_cycle_transport_result_json(
                503,
                c"".as_ptr(),
                c"try later".as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"classification\":\"serverUnavailable\""));
            assert!(output.contains("\"shouldRetry\":true"));

            let required = actro_core_browser_storage_sync_status_policy_json(
                c"credentialsUnavailable".as_ptr(),
                false,
                c"refreshCredentials".as_ptr(),
                0,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"schedulerAction\":\"waitForCredentials\""));
            assert!(output.contains("\"credentialAction\":\"refreshUserSession\""));

            let required = actro_core_browser_storage_sync_schedule_state_json(
                storage_buffer.as_ptr(),
                10_000,
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"schedulerAction\":\"waitForCredentials\""));
            assert!(output.contains("\"waitingForCredentials\":true"));
            assert!(output.contains("\"nextRetryAttempt\":0"));

            let required = actro_core_browser_storage_sync_scheduler_status_json(
                c"{\"retryAttempt\":1,\"schedulerAction\":\"scheduleRetry\",\"nextRunDelaySeconds\":5,\"nextRunAtUnixSeconds\":10005,\"waitingForCredentials\":false,\"requiresUserAction\":false,\"pendingRunScheduled\":true,\"lastResultJson\":\"{\\\"classification\\\":\\\"serverUnavailable\\\"}\"}".as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"status\":\"retryScheduled\""));
            assert!(output.contains("\"lastClassification\":\"serverUnavailable\""));
            assert!(output.contains("\"pendingRunScheduled\":true"));

            let required = actro_core_browser_storage_sync_ack_plan_json(
                c"{\"version\":1,\"storageSchemaVersion\":6,\"scope\":\"profile\",\"deviceId\":\"device-a\",\"syncedAtUnixSeconds\":1,\"records\":[{\"recordType\":\"historyPage\",\"recordId\":\"https://example.com\",\"syncRevision\":1}],\"cursors\":[]}".as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"statementKey\":\"markHistoryPageSynced\""));
            assert!(output.contains("\"params\":[1,1,\"device-a\",\"https://example.com/\",1]"));

            let required = actro_core_browser_storage_sync_remote_apply_plan_json(
                c"{\"version\":1,\"storageSchemaVersion\":6,\"scope\":\"profile\",\"deviceId\":\"device-b\",\"generatedAtUnixSeconds\":2,\"records\":[{\"recordId\":\"ignored\",\"syncRevision\":3,\"syncState\":\"remote\",\"originDeviceId\":\"device-b\",\"updatedAtUnixSeconds\":2,\"recordType\":\"historyPage\",\"payload\":{\"address\":\"https://example.com\",\"title\":\"Example\",\"visitCount\":1,\"lastVisitedAtUnixSeconds\":2,\"createdAtUnixSeconds\":1,\"updatedAtUnixSeconds\":2}}]}".as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"statementKey\":\"applyRemoteHistoryPage\""));
            assert!(output.contains("\"conflictPolicy\":\"skipLocalDirtyOrNewerRevision\""));
            let plan = std::ffi::CString::new(output).unwrap();

            let required = actro_core_browser_storage_sync_remote_apply_result_json(
                plan.as_ptr(),
                c"{\"version\":1,\"storageSchemaVersion\":6,\"scope\":\"profile\",\"deviceId\":\"device-b\",\"generatedAtUnixSeconds\":3,\"results\":[{\"operationIndex\":0,\"rowsAffected\":0}]}".as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"skippedCount\":1"));
            assert!(output.contains("\"reason\":\"localDirtyOrNewerRevision\""));

            let required = actro_core_browser_storage_sync_server_merge_plan_json(
                c"{\"version\":1,\"storageSchemaVersion\":6,\"requestType\":\"push\",\"scope\":\"profile\",\"deviceId\":\"device-a\",\"generatedAtUnixSeconds\":4,\"pushBatch\":{\"version\":1,\"storageSchemaVersion\":6,\"scope\":\"profile\",\"deviceId\":\"device-a\",\"generatedAtUnixSeconds\":4,\"records\":[{\"recordId\":\"ignored\",\"syncRevision\":1,\"syncState\":\"local\",\"originDeviceId\":\"device-a\",\"updatedAtUnixSeconds\":4,\"recordType\":\"historyPage\",\"payload\":{\"address\":\"https://example.com\",\"title\":\"Example\",\"visitCount\":1,\"lastVisitedAtUnixSeconds\":4,\"createdAtUnixSeconds\":1,\"updatedAtUnixSeconds\":4}}]}}".as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"statementKey\":\"mergePushedHistoryPage\""));
            assert!(output.contains("\"mergePolicy\":\"historyPageAggregateByAddress\""));
            let plan = std::ffi::CString::new(output).unwrap();

            let required = actro_core_browser_storage_sync_server_merge_result_json(
                plan.as_ptr(),
                c"{\"version\":1,\"storageSchemaVersion\":6,\"scope\":\"profile\",\"deviceId\":\"device-a\",\"generatedAtUnixSeconds\":5,\"results\":[{\"operationIndex\":0,\"rowsAffected\":1}]}".as_ptr(),
                storage_buffer.as_mut_ptr(),
                storage_buffer.len(),
            );
            assert!(required > 0);
            let output = CStr::from_ptr(storage_buffer.as_ptr()).to_str().unwrap();
            assert!(output.contains("\"acceptedCount\":1"));
            assert!(output.contains("\"ackBatch\""));
        }
    }
}
