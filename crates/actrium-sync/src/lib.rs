//! Reference executor for Actro browser sync.
//!
//! This crate is intentionally outside client apps. Client apps consume
//! `actro-core` contracts, while local probes and tests can use this crate as
//! a SQLite reference executor for those contracts. Product browser sync is
//! expected to use user-owned cloud storage through provider adapters instead
//! of an Actro-operated sync service.

use actro_core::browser_data::{
    self, BROWSER_STORAGE_SCHEMA_VERSION, BROWSER_STORAGE_SYNC_CONTRACT_VERSION,
    BrowserStorageSyncExecutionPlan, BrowserStorageSyncExecutionResultBatch,
    BrowserStorageSyncOperation, BrowserStorageSyncOperationResult,
};
use rusqlite::types::{Value as SqlValue, ValueRef};
use rusqlite::{Connection, Transaction, params_from_iter};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum BrowserSyncServerError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("invalid sync payload: {0}")]
    InvalidPayload(&'static str),
    #[error("unauthorized sync request: {0}")]
    Unauthorized(&'static str),
    #[error("forbidden sync request: {0}")]
    Forbidden(&'static str),
    #[error("sync request limit exceeded: {0}")]
    RequestLimit(&'static str),
    #[error("unsupported storage schema version: expected {expected}, got {actual}")]
    UnsupportedStorageSchemaVersion { expected: u32, actual: u64 },
    #[error("invalid sync json: {0}")]
    InvalidJson(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, BrowserSyncServerError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserSyncRequestContext {
    pub account_id: String,
    pub device_id: String,
    pub access_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserSyncServiceConfig {
    pub max_push_records: usize,
    pub max_records_per_collection: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserSyncHttpRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl BrowserSyncHttpRequest {
    pub fn new<M, P, H, N, V, B>(method: M, path: P, headers: H, body: B) -> Self
    where
        M: Into<String>,
        P: Into<String>,
        H: IntoIterator<Item = (N, V)>,
        N: Into<String>,
        V: Into<String>,
        B: Into<String>,
    {
        Self {
            method: method.into(),
            path: path.into(),
            headers: headers
                .into_iter()
                .map(|(name, value)| (name.into(), value.into()))
                .collect(),
            body: body.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserSyncHttpResponse {
    pub status_code: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Default for BrowserSyncServiceConfig {
    fn default() -> Self {
        Self {
            max_push_records: browser_data::MAX_BROWSER_STORAGE_SYNC_RECORDS,
            max_records_per_collection: browser_data::DEFAULT_SYNC_PULL_LIMIT,
        }
    }
}

pub struct BrowserSyncServerDatabase {
    connection: Connection,
}

pub struct BrowserSyncService {
    database: BrowserSyncServerDatabase,
    config: BrowserSyncServiceConfig,
}

impl BrowserSyncService {
    pub fn open_in_memory(config: BrowserSyncServiceConfig) -> Result<Self> {
        Self::from_database(BrowserSyncServerDatabase::open_in_memory()?, config)
    }

    pub fn from_database(
        database: BrowserSyncServerDatabase,
        config: BrowserSyncServiceConfig,
    ) -> Result<Self> {
        let service = Self { database, config };
        service.initialize()?;
        Ok(service)
    }

    pub fn initialize(&self) -> Result<()> {
        self.database.connection.execute_batch(
            r#"
CREATE TABLE IF NOT EXISTS browser_sync_server_devices (
    scope TEXT NOT NULL,
    device_id TEXT NOT NULL,
    access_token TEXT NOT NULL,
    registered_at INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    disabled_at INTEGER,
    PRIMARY KEY (scope, device_id)
);
CREATE INDEX IF NOT EXISTS idx_browser_sync_server_devices_last_seen
    ON browser_sync_server_devices(scope, last_seen_at DESC);
"#,
        )?;
        Ok(())
    }

    pub fn register_device(
        &mut self,
        account_id: &str,
        device_id: &str,
        access_token: &str,
        registered_at_unix_seconds: i64,
    ) -> Result<BrowserSyncRequestContext> {
        let account_id = normalize_service_id(account_id, "account id")?;
        let device_id = normalize_service_id(device_id, "device id")?;
        let access_token = normalize_access_token(access_token)?;
        let timestamp = registered_at_unix_seconds.max(0);
        self.database.connection.execute(
            r#"
INSERT INTO browser_sync_server_devices (
    scope, device_id, access_token, registered_at, last_seen_at, disabled_at
)
VALUES (?, ?, ?, ?, ?, NULL)
ON CONFLICT(scope, device_id) DO UPDATE SET
    access_token = excluded.access_token,
    last_seen_at = excluded.last_seen_at,
    disabled_at = NULL
"#,
            (&account_id, &device_id, &access_token, timestamp, timestamp),
        )?;
        Ok(BrowserSyncRequestContext {
            account_id,
            device_id,
            access_token,
        })
    }

    pub fn disable_device(
        &mut self,
        account_id: &str,
        device_id: &str,
        disabled_at_unix_seconds: i64,
    ) -> Result<()> {
        let account_id = normalize_service_id(account_id, "account id")?;
        let device_id = normalize_service_id(device_id, "device id")?;
        self.database.connection.execute(
            "UPDATE browser_sync_server_devices SET disabled_at = ? WHERE scope = ? AND device_id = ?",
            (disabled_at_unix_seconds.max(0), account_id, device_id),
        )?;
        Ok(())
    }

    pub fn merge_push_request_json(
        &mut self,
        context: &BrowserSyncRequestContext,
        push_request_json: &str,
    ) -> Result<String> {
        let generated_at = validate_push_request_context(
            push_request_json,
            context,
            self.config.max_push_records,
        )?;
        self.authorize_context(context, generated_at)?;
        self.database.merge_push_request_json(push_request_json)
    }

    pub fn pull_remote_batch_json(
        &mut self,
        context: &BrowserSyncRequestContext,
        generated_at_unix_seconds: i64,
        cursors_json: &str,
    ) -> Result<String> {
        let (account_id, device_id) = self.authorize_context(context, generated_at_unix_seconds)?;
        self.database.pull_remote_batch_json(
            &account_id,
            &device_id,
            generated_at_unix_seconds,
            cursors_json,
            self.config.max_records_per_collection,
        )
    }

    pub fn sync_cycle_json(
        &mut self,
        context: &BrowserSyncRequestContext,
        push_request_json: &str,
        cursors_json: &str,
    ) -> Result<String> {
        let generated_at = validate_push_request_context(
            push_request_json,
            context,
            self.config.max_push_records,
        )?;
        self.authorize_context(context, generated_at)?;
        self.database.sync_cycle_json(
            push_request_json,
            cursors_json,
            self.config.max_records_per_collection,
        )
    }

    pub fn device_cursors_json(
        &mut self,
        context: &BrowserSyncRequestContext,
        observed_at_unix_seconds: i64,
    ) -> Result<String> {
        let (account_id, device_id) = self.authorize_context(context, observed_at_unix_seconds)?;
        self.database.device_cursors_json(&account_id, &device_id)
    }

    pub fn handle_http_request(
        &mut self,
        request: BrowserSyncHttpRequest,
    ) -> BrowserSyncHttpResponse {
        let method = request.method.trim().to_ascii_uppercase();
        let path = request
            .path
            .split_once('?')
            .map(|(path, _)| path)
            .unwrap_or(request.path.as_str());

        if method == "GET" && path == browser_data::BROWSER_STORAGE_SYNC_TRANSPORT_BASE_PATH {
            return sync_http_json_response(
                200,
                browser_data::browser_storage_sync_transport_contract_json(),
            );
        }

        let context = match sync_http_context(&request.headers) {
            Ok(context) => context,
            Err(response) => return response,
        };

        match (method.as_str(), path) {
            ("POST", browser_data::BROWSER_STORAGE_SYNC_TRANSPORT_PUSH_PATH) => {
                if let Err(response) = require_json_content_type(&request.headers) {
                    return response;
                }
                match self.merge_push_request_json(&context, &request.body) {
                    Ok(body) => sync_http_json_response(200, body),
                    Err(error) => sync_http_error_response(error),
                }
            }
            ("POST", browser_data::BROWSER_STORAGE_SYNC_TRANSPORT_PULL_PATH) => {
                if let Err(response) = require_json_content_type(&request.headers) {
                    return response;
                }
                match sync_http_pull_request_parts(&request.body, &context).and_then(
                    |(generated_at, cursors_json)| {
                        self.pull_remote_batch_json(&context, generated_at, &cursors_json)
                    },
                ) {
                    Ok(body) => sync_http_json_response(200, body),
                    Err(error) => sync_http_error_response(error),
                }
            }
            ("POST", browser_data::BROWSER_STORAGE_SYNC_TRANSPORT_CYCLE_PATH) => {
                if let Err(response) = require_json_content_type(&request.headers) {
                    return response;
                }
                match sync_http_cycle_request_parts(&request.body).and_then(
                    |(push_request_json, cursors_json)| {
                        self.sync_cycle_json(&context, &push_request_json, &cursors_json)
                    },
                ) {
                    Ok(body) => sync_http_json_response(200, body),
                    Err(error) => sync_http_error_response(error),
                }
            }
            ("GET", browser_data::BROWSER_STORAGE_SYNC_TRANSPORT_CURSORS_PATH) => {
                match self.device_cursors_json(&context, current_unix_seconds()) {
                    Ok(body) => sync_http_json_response(200, body),
                    Err(error) => sync_http_error_response(error),
                }
            }
            (_, browser_data::BROWSER_STORAGE_SYNC_TRANSPORT_PUSH_PATH)
            | (_, browser_data::BROWSER_STORAGE_SYNC_TRANSPORT_PULL_PATH)
            | (_, browser_data::BROWSER_STORAGE_SYNC_TRANSPORT_CYCLE_PATH)
            | (_, browser_data::BROWSER_STORAGE_SYNC_TRANSPORT_CURSORS_PATH)
            | (_, browser_data::BROWSER_STORAGE_SYNC_TRANSPORT_BASE_PATH) => {
                sync_http_error_json_response(405, "methodNotAllowed", "method not allowed")
            }
            _ => sync_http_error_json_response(404, "notFound", "unknown sync endpoint"),
        }
    }

    fn authorize_context(
        &mut self,
        context: &BrowserSyncRequestContext,
        observed_at_unix_seconds: i64,
    ) -> Result<(String, String)> {
        let account_id = normalize_service_id(&context.account_id, "account id")?;
        let device_id = normalize_service_id(&context.device_id, "device id")?;
        let access_token = normalize_access_token(&context.access_token)?;

        let row = self
            .database
            .connection
            .query_row(
                "SELECT access_token, disabled_at FROM browser_sync_server_devices WHERE scope = ? AND device_id = ?",
                (&account_id, &device_id),
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?)),
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => {
                    BrowserSyncServerError::Unauthorized("unknown device")
                }
                other => BrowserSyncServerError::Sqlite(other),
            })?;

        if row.1.is_some() {
            return Err(BrowserSyncServerError::Unauthorized("disabled device"));
        }
        if row.0 != access_token {
            return Err(BrowserSyncServerError::Unauthorized("token mismatch"));
        }

        self.database.connection.execute(
            "UPDATE browser_sync_server_devices SET last_seen_at = ? WHERE scope = ? AND device_id = ?",
            (observed_at_unix_seconds.max(0), &account_id, &device_id),
        )?;
        Ok((account_id, device_id))
    }
}

impl BrowserSyncServerDatabase {
    pub fn open_in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_connection(Connection::open(path)?)
    }

    pub fn from_connection(connection: Connection) -> Result<Self> {
        let database = Self { connection };
        database.initialize()?;
        Ok(database)
    }

    pub fn initialize(&self) -> Result<()> {
        for statement in browser_data::browser_storage_sync_server_schema_statements() {
            self.connection.execute_batch(statement)?;
        }
        Ok(())
    }

    pub fn merge_push_request_json(&mut self, push_request_json: &str) -> Result<String> {
        let plan_json =
            browser_data::browser_storage_sync_server_merge_plan_json(push_request_json);
        if plan_json == "null" {
            return Err(BrowserSyncServerError::InvalidPayload("push request"));
        }

        let plan: BrowserStorageSyncExecutionPlan = serde_json::from_str(&plan_json)?;
        let generated_at = plan.generated_at_unix_seconds;
        let result_batch = self.execute_merge_plan(&plan, generated_at)?;
        let result_json = serde_json::to_string(&result_batch)?;
        let summary_json =
            browser_data::browser_storage_sync_server_merge_result_json(&plan_json, &result_json);
        if summary_json == "null" {
            return Err(BrowserSyncServerError::InvalidPayload(
                "server merge result",
            ));
        }
        Ok(summary_json)
    }

    pub fn pull_remote_batch_json(
        &self,
        scope: &str,
        device_id: &str,
        generated_at_unix_seconds: i64,
        cursors_json: &str,
        max_records_per_collection: u32,
    ) -> Result<String> {
        let cursors = normalize_cursor_map(cursors_json)?;
        let limit = if max_records_per_collection == 0 {
            browser_data::DEFAULT_SYNC_PULL_LIMIT
        } else {
            max_records_per_collection
        };
        let mut rows = Vec::new();

        for (collection, sql) in remote_select_statements() {
            let cursor = cursors.get(collection).map(String::as_str).unwrap_or("");
            rows.extend(self.query_remote_rows(sql, scope, cursor, device_id, limit)?);
        }

        let rows_json = serde_json::to_string(&rows)?;
        let batch_json = browser_data::browser_storage_sync_batch_from_dirty_rows_json(
            scope,
            device_id,
            generated_at_unix_seconds,
            &rows_json,
        );
        if batch_json == "null" {
            return Err(BrowserSyncServerError::InvalidPayload("remote rows"));
        }
        Ok(batch_json)
    }

    pub fn sync_cycle_json(
        &mut self,
        push_request_json: &str,
        cursors_json: &str,
        max_records_per_collection: u32,
    ) -> Result<String> {
        let summary_json = self.merge_push_request_json(push_request_json)?;
        let summary: Value = serde_json::from_str(&summary_json)?;
        let scope = summary
            .get("scope")
            .and_then(Value::as_str)
            .ok_or(BrowserSyncServerError::InvalidPayload("summary scope"))?;
        let device_id = summary
            .get("deviceId")
            .and_then(Value::as_str)
            .ok_or(BrowserSyncServerError::InvalidPayload("summary device id"))?;
        let generated_at = summary
            .get("generatedAtUnixSeconds")
            .and_then(Value::as_i64)
            .ok_or(BrowserSyncServerError::InvalidPayload("summary timestamp"))?;
        let ack_json = serde_json::to_string(
            summary
                .get("ackBatch")
                .ok_or(BrowserSyncServerError::InvalidPayload("summary ack batch"))?,
        )?;
        let remote_batch_json = self.pull_remote_batch_json(
            scope,
            device_id,
            generated_at,
            cursors_json,
            max_records_per_collection,
        )?;
        let response_json =
            browser_data::browser_storage_sync_cycle_response_json(&remote_batch_json, &ack_json);
        if response_json == "null" {
            return Err(BrowserSyncServerError::InvalidPayload("sync cycle"));
        }
        Ok(response_json)
    }

    pub fn device_cursors_json(&self, scope: &str, device_id: &str) -> Result<String> {
        let sql = browser_data::browser_storage_sync_server_sql().select_device_cursors;
        let mut statement = self.connection.prepare(sql)?;
        let cursors = statement
            .query_map([scope, device_id], |row| {
                Ok(json!({
                    "collection": row.get::<_, String>(0)?,
                    "cursor": row.get::<_, String>(1)?,
                    "lastSyncedAtUnixSeconds": row.get::<_, Option<i64>>(2)?,
                }))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(browser_data::normalize_browser_storage_sync_cursors_json(
            &serde_json::to_string(&cursors)?,
        ))
    }

    fn execute_merge_plan(
        &mut self,
        plan: &BrowserStorageSyncExecutionPlan,
        generated_at: i64,
    ) -> Result<BrowserStorageSyncExecutionResultBatch> {
        let tx = self.connection.transaction()?;
        let mut results = Vec::new();
        let mut max_revision_by_collection = BTreeMap::<String, i64>::new();

        for (operation_index, operation) in plan.operations.iter().enumerate() {
            match execute_merge_operation(&tx, operation, generated_at) {
                Ok((rows_affected, server_revision)) => {
                    if rows_affected > 0 {
                        max_revision_by_collection
                            .entry(operation.collection.clone())
                            .and_modify(|revision| *revision = (*revision).max(server_revision))
                            .or_insert(server_revision);
                    }
                    results.push(BrowserStorageSyncOperationResult {
                        operation_index,
                        rows_affected,
                        sync_revision: Some(server_revision),
                        error: None,
                    });
                }
                Err(error) => {
                    results.push(BrowserStorageSyncOperationResult {
                        operation_index,
                        rows_affected: 0,
                        sync_revision: None,
                        error: Some(error.to_string()),
                    });
                }
            }
        }

        for (collection, revision) in &max_revision_by_collection {
            upsert_device_cursor(
                &tx,
                &plan.scope,
                &plan.device_id,
                collection,
                *revision,
                generated_at,
            )?;
        }

        tx.commit()?;

        let cursors = max_revision_by_collection
            .into_iter()
            .map(
                |(collection, revision)| browser_data::BrowserStorageSyncCursorUpdate {
                    collection,
                    cursor: revision.to_string(),
                    last_synced_at_unix_seconds: Some(generated_at),
                },
            )
            .collect();

        Ok(BrowserStorageSyncExecutionResultBatch {
            version: BROWSER_STORAGE_SYNC_CONTRACT_VERSION,
            storage_schema_version: BROWSER_STORAGE_SCHEMA_VERSION,
            scope: plan.scope.clone(),
            device_id: plan.device_id.clone(),
            generated_at_unix_seconds: generated_at,
            results,
            cursors,
        })
    }

    fn query_remote_rows(
        &self,
        sql: &str,
        scope: &str,
        cursor: &str,
        device_id: &str,
        limit: u32,
    ) -> Result<Vec<Value>> {
        let mut statement = self.connection.prepare(sql)?;
        let names = statement
            .column_names()
            .into_iter()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let rows = statement
            .query_map(
                (
                    scope,
                    cursor,
                    device_id,
                    i64::from(limit.min(browser_data::MAX_BROWSER_STORAGE_SYNC_RECORDS as u32)),
                ),
                |row| {
                    let mut object = serde_json::Map::new();
                    for (index, name) in names.iter().enumerate() {
                        object.insert(name.clone(), sqlite_value_to_json(row.get_ref(index)?));
                    }
                    Ok(Value::Object(object))
                },
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

fn execute_merge_operation(
    tx: &Transaction<'_>,
    operation: &BrowserStorageSyncOperation,
    generated_at: i64,
) -> Result<(i64, i64)> {
    let server_revision = allocate_server_revision(tx, operation, generated_at)?;
    let mut params = Vec::with_capacity(operation.params.len() + 1);
    params.push(SqlValue::Integer(server_revision));
    params.extend(operation.params.iter().map(json_to_sql_value));
    let statement = merge_statement(operation.statement_key.as_str())?;
    let rows_affected = tx.execute(statement, params_from_iter(params.iter()))? as i64;
    Ok((rows_affected, server_revision))
}

fn allocate_server_revision(
    tx: &Transaction<'_>,
    operation: &BrowserStorageSyncOperation,
    generated_at: i64,
) -> Result<i64> {
    let record_type =
        operation
            .record_type
            .as_deref()
            .ok_or(BrowserSyncServerError::InvalidPayload(
                "operation record type",
            ))?;
    let record_id =
        operation
            .record_id
            .as_deref()
            .ok_or(BrowserSyncServerError::InvalidPayload(
                "operation record id",
            ))?;
    let origin_device_id = operation
        .params
        .get(1)
        .and_then(Value::as_str)
        .unwrap_or("unknown-device");
    let client_sync_revision = operation.params.get(3).and_then(Value::as_i64).unwrap_or(0);
    let sql = browser_data::browser_storage_sync_server_sql().allocate_server_revision;
    let server_revision = tx.query_row(
        sql,
        (
            operation
                .params
                .first()
                .and_then(Value::as_str)
                .unwrap_or("default"),
            operation.collection.as_str(),
            record_type,
            record_id,
            origin_device_id,
            client_sync_revision,
            generated_at,
        ),
        |row| row.get(0),
    )?;
    Ok(server_revision)
}

fn upsert_device_cursor(
    tx: &Transaction<'_>,
    scope: &str,
    device_id: &str,
    collection: &str,
    revision: i64,
    synced_at: i64,
) -> Result<()> {
    let sql = browser_data::browser_storage_sync_server_sql().upsert_device_cursor;
    tx.execute(
        sql,
        (
            scope,
            device_id,
            collection,
            revision.to_string(),
            synced_at,
            synced_at,
        ),
    )?;
    Ok(())
}

fn merge_statement(statement_key: &str) -> Result<&'static str> {
    let sql = browser_data::browser_storage_sync_server_sql();
    match statement_key {
        "mergePushedHistoryPage" => Ok(sql.merge_pushed_history_page),
        "mergePushedHistoryVisit" => Ok(sql.merge_pushed_history_visit),
        "mergePushedBookmarkFolder" => Ok(sql.merge_pushed_bookmark_folder),
        "mergePushedBookmark" => Ok(sql.merge_pushed_bookmark),
        "mergePushedSession" => Ok(sql.merge_pushed_session),
        "mergePushedSessionTab" => Ok(sql.merge_pushed_session_tab),
        _ => Err(BrowserSyncServerError::InvalidPayload(
            "merge statement key",
        )),
    }
}

fn remote_select_statements() -> [(&'static str, &'static str); 6] {
    let sql = browser_data::browser_storage_sync_server_sql();
    [
        ("historyPages", sql.select_remote_history_pages),
        ("historyVisits", sql.select_remote_history_visits),
        ("bookmarkFolders", sql.select_remote_bookmark_folders),
        ("bookmarks", sql.select_remote_bookmarks),
        ("sessions", sql.select_remote_sessions),
        ("sessionTabs", sql.select_remote_session_tabs),
    ]
}

fn normalize_cursor_map(cursors_json: &str) -> Result<BTreeMap<String, String>> {
    let normalized = browser_data::normalize_browser_storage_sync_cursors_json(cursors_json);
    if normalized == "null" {
        return Err(BrowserSyncServerError::InvalidPayload("cursors"));
    }
    let values: Vec<Value> = serde_json::from_str(&normalized)?;
    Ok(values
        .into_iter()
        .filter_map(|value| {
            Some((
                value.get("collection")?.as_str()?.to_string(),
                value.get("cursor")?.as_str()?.to_string(),
            ))
        })
        .collect())
}

fn validate_storage_schema_version(value: &Value, label: &'static str) -> Result<()> {
    let actual = value
        .get("storageSchemaVersion")
        .and_then(Value::as_u64)
        .ok_or(BrowserSyncServerError::InvalidPayload(label))?;
    if actual != u64::from(BROWSER_STORAGE_SCHEMA_VERSION) {
        return Err(BrowserSyncServerError::UnsupportedStorageSchemaVersion {
            expected: BROWSER_STORAGE_SCHEMA_VERSION,
            actual,
        });
    }
    Ok(())
}

fn validate_push_request_context(
    push_request_json: &str,
    context: &BrowserSyncRequestContext,
    max_push_records: usize,
) -> Result<i64> {
    let value: Value = serde_json::from_str(push_request_json)?;
    validate_storage_schema_version(&value, "push storage schema version")?;
    let context_account = normalize_service_id(&context.account_id, "account id")?;
    let context_device = normalize_service_id(&context.device_id, "device id")?;
    let scope = value
        .get("scope")
        .and_then(Value::as_str)
        .ok_or(BrowserSyncServerError::InvalidPayload("push scope"))
        .and_then(|value| normalize_service_id(value, "push scope"))?;
    let device_id = value
        .get("deviceId")
        .and_then(Value::as_str)
        .ok_or(BrowserSyncServerError::InvalidPayload("push device id"))
        .and_then(|value| normalize_service_id(value, "push device id"))?;

    if scope != context_account {
        return Err(BrowserSyncServerError::Forbidden("scope mismatch"));
    }
    if device_id != context_device {
        return Err(BrowserSyncServerError::Forbidden("device mismatch"));
    }

    if let Some(request_type) = value.get("requestType").and_then(Value::as_str)
        && request_type.trim() != "push"
    {
        return Err(BrowserSyncServerError::InvalidPayload("request type"));
    }

    let push_batch = value
        .get("pushBatch")
        .ok_or(BrowserSyncServerError::InvalidPayload("push batch"))?;
    validate_storage_schema_version(push_batch, "push batch storage schema version")?;
    if let Some(batch_scope) = push_batch.get("scope").and_then(Value::as_str)
        && normalize_service_id(batch_scope, "push batch scope")? != context_account
    {
        return Err(BrowserSyncServerError::Forbidden(
            "push batch scope mismatch",
        ));
    }
    if let Some(batch_device_id) = push_batch.get("deviceId").and_then(Value::as_str)
        && normalize_service_id(batch_device_id, "push batch device id")? != context_device
    {
        return Err(BrowserSyncServerError::Forbidden(
            "push batch device mismatch",
        ));
    }

    let records = push_batch
        .get("records")
        .and_then(Value::as_array)
        .ok_or(BrowserSyncServerError::InvalidPayload("push records"))?;
    if records.len() > max_push_records {
        return Err(BrowserSyncServerError::RequestLimit("push records"));
    }

    Ok(value
        .get("generatedAtUnixSeconds")
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .max(0))
}

fn normalize_service_id(value: &str, label: &'static str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(BrowserSyncServerError::InvalidPayload(label));
    }
    if value.len() > browser_data::MAX_SYNC_ID_LENGTH {
        return Err(BrowserSyncServerError::InvalidPayload(label));
    }
    Ok(value.to_string())
}

fn normalize_access_token(value: &str) -> Result<String> {
    let value = value.trim();
    if value.len() < 8 || value.len() > 256 {
        return Err(BrowserSyncServerError::InvalidPayload("access token"));
    }
    Ok(value.to_string())
}

fn sync_http_context(
    headers: &[(String, String)],
) -> std::result::Result<BrowserSyncRequestContext, BrowserSyncHttpResponse> {
    let account_id =
        sync_http_required_header(headers, browser_data::BROWSER_STORAGE_SYNC_ACCOUNT_HEADER)
            .and_then(|value| {
                normalize_service_id(value, "account id").map_err(sync_http_error_response)
            })?;
    let device_id =
        sync_http_required_header(headers, browser_data::BROWSER_STORAGE_SYNC_DEVICE_HEADER)
            .and_then(|value| {
                normalize_service_id(value, "device id").map_err(sync_http_error_response)
            })?;
    let authorization = sync_http_required_header(
        headers,
        browser_data::BROWSER_STORAGE_SYNC_AUTHORIZATION_HEADER,
    )?;
    let access_token = sync_http_bearer_token(authorization)?;

    Ok(BrowserSyncRequestContext {
        account_id,
        device_id,
        access_token,
    })
}

fn sync_http_required_header<'a>(
    headers: &'a [(String, String)],
    name: &str,
) -> std::result::Result<&'a str, BrowserSyncHttpResponse> {
    sync_http_header(headers, name)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| sync_http_error_json_response(401, "missingHeader", name))
}

fn sync_http_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .rev()
        .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn sync_http_bearer_token(
    authorization: &str,
) -> std::result::Result<String, BrowserSyncHttpResponse> {
    let authorization = authorization.trim();
    let Some((scheme, token)) = authorization.split_once(char::is_whitespace) else {
        return Err(sync_http_error_json_response(
            401,
            "invalidAuthorization",
            "expected Bearer token",
        ));
    };
    if !scheme.eq_ignore_ascii_case("Bearer") {
        return Err(sync_http_error_json_response(
            401,
            "invalidAuthorization",
            "expected Bearer token",
        ));
    }
    normalize_access_token(token).map_err(sync_http_error_response)
}

fn require_json_content_type(
    headers: &[(String, String)],
) -> std::result::Result<(), BrowserSyncHttpResponse> {
    let content_type = sync_http_header(headers, "Content-Type")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            sync_http_error_json_response(415, "unsupportedMediaType", "expected application/json")
        })?;
    let media_type = content_type.split(';').next().unwrap_or_default().trim();
    if media_type.eq_ignore_ascii_case(browser_data::BROWSER_STORAGE_SYNC_CONTENT_TYPE) {
        Ok(())
    } else {
        Err(sync_http_error_json_response(
            415,
            "unsupportedMediaType",
            "expected application/json",
        ))
    }
}

fn sync_http_pull_request_parts(
    body: &str,
    context: &BrowserSyncRequestContext,
) -> Result<(i64, String)> {
    let value: Value = serde_json::from_str(body)?;
    validate_storage_schema_version(&value, "pull storage schema version")?;
    if let Some(request_type) = value.get("requestType").and_then(Value::as_str)
        && request_type.trim() != "pull"
    {
        return Err(BrowserSyncServerError::InvalidPayload("pull request type"));
    }

    let scope = value
        .get("scope")
        .and_then(Value::as_str)
        .ok_or(BrowserSyncServerError::InvalidPayload("pull scope"))
        .and_then(|value| normalize_service_id(value, "pull scope"))?;
    let device_id = value
        .get("deviceId")
        .and_then(Value::as_str)
        .ok_or(BrowserSyncServerError::InvalidPayload("pull device id"))
        .and_then(|value| normalize_service_id(value, "pull device id"))?;
    if scope != context.account_id {
        return Err(BrowserSyncServerError::Forbidden("scope mismatch"));
    }
    if device_id != context.device_id {
        return Err(BrowserSyncServerError::Forbidden("device mismatch"));
    }

    let generated_at = value
        .get("generatedAtUnixSeconds")
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .max(0);
    let cursors_json =
        serde_json::to_string(value.get("cursors").unwrap_or(&Value::Array(Vec::new())))?;
    Ok((generated_at, cursors_json))
}

fn sync_http_cycle_request_parts(body: &str) -> Result<(String, String)> {
    let value: Value = serde_json::from_str(body)?;
    let push_request_json = serde_json::to_string(
        value
            .get("pushRequest")
            .ok_or(BrowserSyncServerError::InvalidPayload("cycle push request"))?,
    )?;
    let cursors_json =
        serde_json::to_string(value.get("cursors").unwrap_or(&Value::Array(Vec::new())))?;
    Ok((push_request_json, cursors_json))
}

fn sync_http_json_response(status_code: u16, body: String) -> BrowserSyncHttpResponse {
    BrowserSyncHttpResponse {
        status_code,
        headers: vec![(
            "Content-Type".to_string(),
            browser_data::BROWSER_STORAGE_SYNC_CONTENT_TYPE.to_string(),
        )],
        body,
    }
}

fn sync_http_error_response(error: BrowserSyncServerError) -> BrowserSyncHttpResponse {
    match error {
        BrowserSyncServerError::Unauthorized(message) => {
            sync_http_error_json_response(401, "unauthorized", message)
        }
        BrowserSyncServerError::Forbidden(message) => {
            sync_http_error_json_response(403, "forbidden", message)
        }
        BrowserSyncServerError::RequestLimit(message) => {
            sync_http_error_json_response(413, "requestLimitExceeded", message)
        }
        BrowserSyncServerError::UnsupportedStorageSchemaVersion { expected, actual } => {
            sync_http_error_json_response(
                426,
                browser_data::BROWSER_STORAGE_SYNC_UNSUPPORTED_SCHEMA_ERROR_CODE,
                format!("expected storage schema version {expected}, got {actual}"),
            )
        }
        BrowserSyncServerError::InvalidPayload(message) => {
            sync_http_error_json_response(400, "invalidPayload", message)
        }
        BrowserSyncServerError::InvalidJson(error) => {
            sync_http_error_json_response(400, "invalidJson", error.to_string())
        }
        BrowserSyncServerError::Sqlite(error) => {
            sync_http_error_json_response(500, "serverStorageError", error.to_string())
        }
    }
}

fn sync_http_error_json_response(
    status_code: u16,
    code: &str,
    message: impl AsRef<str>,
) -> BrowserSyncHttpResponse {
    sync_http_json_response(
        status_code,
        json!({
            "error": {
                "code": code,
                "message": message.as_ref(),
            }
        })
        .to_string(),
    )
}

fn current_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn json_to_sql_value(value: &Value) -> SqlValue {
    match value {
        Value::Null => SqlValue::Null,
        Value::Bool(value) => SqlValue::Integer(i64::from(*value)),
        Value::Number(value) => value
            .as_i64()
            .map(SqlValue::Integer)
            .or_else(|| value.as_f64().map(SqlValue::Real))
            .unwrap_or(SqlValue::Null),
        Value::String(value) => SqlValue::Text(value.clone()),
        Value::Array(_) | Value::Object(_) => SqlValue::Text(value.to_string()),
    }
}

fn sqlite_value_to_json(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(value) => json!(value),
        ValueRef::Real(value) => json!(value),
        ValueRef::Text(value) => json!(String::from_utf8_lossy(value).to_string()),
        ValueRef::Blob(value) => json!(value.to_vec()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_request(
        scope: &str,
        device_id: &str,
        generated_at: i64,
        records: Vec<Value>,
    ) -> String {
        json!({
            "version": 1,
            "storageSchemaVersion": 6,
            "requestType": "push",
            "scope": scope,
            "deviceId": device_id,
            "generatedAtUnixSeconds": generated_at,
            "pushBatch": {
                "version": 1,
                "storageSchemaVersion": 6,
                "scope": scope,
                "deviceId": device_id,
                "generatedAtUnixSeconds": generated_at,
                "records": records
            }
        })
        .to_string()
    }

    fn history_record(device_id: &str, address: &str, updated_at: i64) -> Value {
        json!({
            "recordId": "ignored",
            "syncRevision": 1,
            "syncState": "local",
            "originDeviceId": device_id,
            "updatedAtUnixSeconds": updated_at,
            "recordType": "historyPage",
            "payload": {
                "address": address,
                "title": address,
                "visitCount": 1,
                "lastVisitedAtUnixSeconds": updated_at,
                "createdAtUnixSeconds": updated_at - 1,
                "updatedAtUnixSeconds": updated_at
            }
        })
    }

    fn bookmark_record(device_id: &str, id: &str, address: &str, updated_at: i64) -> Value {
        json!({
            "recordId": "ignored",
            "syncRevision": 1,
            "syncState": "local",
            "originDeviceId": device_id,
            "updatedAtUnixSeconds": updated_at,
            "recordType": "bookmark",
            "payload": {
                "id": id,
                "folderId": null,
                "title": address,
                "address": address,
                "createdAtUnixSeconds": updated_at - 1,
                "updatedAtUnixSeconds": updated_at,
                "sortOrder": 0
            }
        })
    }

    fn sync_http_headers(account_id: &str, device_id: &str, token: &str) -> Vec<(String, String)> {
        vec![
            (
                browser_data::BROWSER_STORAGE_SYNC_AUTHORIZATION_HEADER.to_string(),
                format!("Bearer {token}"),
            ),
            (
                browser_data::BROWSER_STORAGE_SYNC_ACCOUNT_HEADER.to_string(),
                account_id.to_string(),
            ),
            (
                browser_data::BROWSER_STORAGE_SYNC_DEVICE_HEADER.to_string(),
                device_id.to_string(),
            ),
            (
                "Content-Type".to_string(),
                browser_data::BROWSER_STORAGE_SYNC_CONTENT_TYPE.to_string(),
            ),
            (
                "Accept".to_string(),
                browser_data::BROWSER_STORAGE_SYNC_CONTENT_TYPE.to_string(),
            ),
        ]
    }

    #[test]
    fn sync_cycle_merges_push_and_pulls_remote_rows() {
        let mut database = BrowserSyncServerDatabase::open_in_memory().unwrap();

        let device_a_push = r#"{
            "version": 1,
            "storageSchemaVersion": 6,
            "requestType": "push",
            "scope": "profile-a",
            "deviceId": "device-a",
            "generatedAtUnixSeconds": 100,
            "pushBatch": {
                "version": 1,
                "storageSchemaVersion": 6,
                "scope": "profile-a",
                "deviceId": "device-a",
                "generatedAtUnixSeconds": 100,
                "records": [
                    {
                        "recordId": "ignored",
                        "syncRevision": 1,
                        "syncState": "local",
                        "originDeviceId": "device-a",
                        "updatedAtUnixSeconds": 100,
                        "recordType": "historyPage",
                        "payload": {
                            "address": "https://example.com/a",
                            "title": "A",
                            "visitCount": 1,
                            "lastVisitedAtUnixSeconds": 100,
                            "createdAtUnixSeconds": 90,
                            "updatedAtUnixSeconds": 100
                        }
                    }
                ]
            }
        }"#;

        let device_a_summary = database.merge_push_request_json(device_a_push).unwrap();
        let summary = serde_json::from_str::<Value>(&device_a_summary).unwrap();
        assert_eq!(summary["acceptedCount"], json!(1));
        assert_eq!(summary["ackBatch"]["records"][0]["syncRevision"], json!(1));
        assert_eq!(summary["ackBatch"]["cursors"][0]["cursor"], json!("1"));

        let device_b_cycle = database
            .sync_cycle_json(
                r#"{
                    "version": 1,
                    "storageSchemaVersion": 6,
                    "requestType": "push",
                    "scope": "profile-a",
                    "deviceId": "device-b",
                    "generatedAtUnixSeconds": 110,
                    "pushBatch": {
                        "version": 1,
                        "storageSchemaVersion": 6,
                        "scope": "profile-a",
                        "deviceId": "device-b",
                        "generatedAtUnixSeconds": 110,
                        "records": [
                            {
                                "recordId": "ignored",
                                "syncRevision": 1,
                                "syncState": "local",
                                "originDeviceId": "device-b",
                                "updatedAtUnixSeconds": 110,
                                "recordType": "bookmark",
                                "payload": {
                                    "id": "bookmark-1",
                                    "folderId": null,
                                    "title": "B",
                                    "address": "https://example.com/b",
                                    "createdAtUnixSeconds": 105,
                                    "updatedAtUnixSeconds": 110,
                                    "sortOrder": 0
                                }
                            }
                        ]
                    }
                }"#,
                r#"[]"#,
                100,
            )
            .unwrap();
        let cycle = serde_json::from_str::<Value>(&device_b_cycle).unwrap();
        assert_eq!(cycle["responseType"], json!("syncCycle"));
        assert_eq!(
            cycle["ackBatch"]["records"][0]["recordType"],
            json!("bookmark")
        );
        assert_eq!(cycle["ackBatch"]["records"][0]["syncRevision"], json!(2));

        let remote_records = cycle["remoteBatch"]["records"].as_array().unwrap();
        assert_eq!(remote_records.len(), 1);
        assert_eq!(remote_records[0]["recordType"], json!("historyPage"));
        assert_eq!(remote_records[0]["syncRevision"], json!(1));
        assert_eq!(remote_records[0]["originDeviceId"], json!("device-a"));
        assert_eq!(
            cycle["remoteBatch"]["cursors"][0]["collection"],
            json!("historyPages")
        );
        assert_eq!(cycle["remoteBatch"]["cursors"][0]["cursor"], json!("1"));
        assert_eq!(
            cycle["ackBatch"]["cursors"][0]["collection"],
            json!("bookmarks")
        );
        assert_eq!(cycle["ackBatch"]["cursors"][0]["cursor"], json!("2"));
        assert_eq!(
            cycle["ackBatch"]["cursors"][1]["collection"],
            json!("historyPages")
        );
        assert_eq!(cycle["ackBatch"]["cursors"][1]["cursor"], json!("1"));

        let device_a_pull = database
            .pull_remote_batch_json(
                "profile-a",
                "device-a",
                120,
                r#"[{"collection":"bookmarks","cursor":"0"}]"#,
                100,
            )
            .unwrap();
        let batch = serde_json::from_str::<Value>(&device_a_pull).unwrap();
        let records = batch["records"].as_array().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["recordType"], json!("bookmark"));
        assert_eq!(records[0]["syncRevision"], json!(2));
        assert_eq!(records[0]["originDeviceId"], json!("device-b"));
        assert_eq!(batch["cursors"][0]["collection"], json!("bookmarks"));
        assert_eq!(batch["cursors"][0]["cursor"], json!("2"));

        let device_b_cursors = database
            .device_cursors_json("profile-a", "device-b")
            .unwrap();
        assert!(device_b_cursors.contains(r#""collection":"bookmarks""#));
        assert!(device_b_cursors.contains(r#""cursor":"2""#));
    }

    #[test]
    fn pull_respects_collection_cursors_and_origin_device_filter() {
        let mut database = BrowserSyncServerDatabase::open_in_memory().unwrap();

        database
            .merge_push_request_json(
                r#"{
                    "version": 1,
                    "storageSchemaVersion": 6,
                    "requestType": "push",
                    "scope": "profile-a",
                    "deviceId": "device-a",
                    "generatedAtUnixSeconds": 100,
                    "pushBatch": {
                        "version": 1,
                        "storageSchemaVersion": 6,
                        "scope": "profile-a",
                        "deviceId": "device-a",
                        "generatedAtUnixSeconds": 100,
                        "records": [
                            {
                                "recordId": "ignored",
                                "syncRevision": 1,
                                "syncState": "local",
                                "originDeviceId": "device-a",
                                "updatedAtUnixSeconds": 100,
                                "recordType": "historyPage",
                                "payload": {
                                    "address": "https://example.com/a",
                                    "title": "A",
                                    "visitCount": 1,
                                    "lastVisitedAtUnixSeconds": 100,
                                    "createdAtUnixSeconds": 90,
                                    "updatedAtUnixSeconds": 100
                                }
                            },
                            {
                                "recordId": "ignored",
                                "syncRevision": 1,
                                "syncState": "local",
                                "originDeviceId": "device-a",
                                "updatedAtUnixSeconds": 101,
                                "recordType": "historyPage",
                                "payload": {
                                    "address": "https://example.com/b",
                                    "title": "B",
                                    "visitCount": 1,
                                    "lastVisitedAtUnixSeconds": 101,
                                    "createdAtUnixSeconds": 91,
                                    "updatedAtUnixSeconds": 101
                                }
                            }
                        ]
                    }
                }"#,
            )
            .unwrap();

        let device_a_pull = database
            .pull_remote_batch_json("profile-a", "device-a", 120, r#"[]"#, 100)
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&device_a_pull).unwrap()["records"]
                .as_array()
                .unwrap()
                .len(),
            0
        );

        let device_b_pull = database
            .pull_remote_batch_json(
                "profile-a",
                "device-b",
                120,
                r#"[{"collection":"historyPages","cursor":"1"}]"#,
                100,
            )
            .unwrap();
        let batch = serde_json::from_str::<Value>(&device_b_pull).unwrap();
        let records = batch["records"].as_array().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0]["payload"]["address"],
            json!("https://example.com/b")
        );
        assert_eq!(records[0]["syncRevision"], json!(2));
        assert_eq!(batch["cursors"][0]["collection"], json!("historyPages"));
        assert_eq!(batch["cursors"][0]["cursor"], json!("2"));
    }

    #[test]
    fn http_facade_routes_contract_push_cycle_pull_and_cursors() {
        let mut service = BrowserSyncService::open_in_memory(BrowserSyncServiceConfig {
            max_push_records: 10,
            max_records_per_collection: 100,
        })
        .unwrap();
        let device_a = service
            .register_device("profile-a", "device-a", "token-a-123", 100)
            .unwrap();
        let device_b = service
            .register_device("profile-a", "device-b", "token-b-123", 100)
            .unwrap();

        let contract_response = service.handle_http_request(BrowserSyncHttpRequest::new(
            "GET",
            browser_data::BROWSER_STORAGE_SYNC_TRANSPORT_BASE_PATH,
            Vec::<(String, String)>::new(),
            "",
        ));
        assert_eq!(contract_response.status_code, 200);
        assert!(
            contract_response
                .body
                .contains(browser_data::BROWSER_STORAGE_SYNC_TRANSPORT_CYCLE_PATH)
        );

        let device_a_push = push_request(
            "profile-a",
            "device-a",
            110,
            vec![history_record(
                "device-a",
                "https://example.com/from-a",
                110,
            )],
        );
        let push_response = service.handle_http_request(BrowserSyncHttpRequest::new(
            "POST",
            browser_data::BROWSER_STORAGE_SYNC_TRANSPORT_PUSH_PATH,
            sync_http_headers(
                &device_a.account_id,
                &device_a.device_id,
                &device_a.access_token,
            ),
            device_a_push,
        ));
        assert_eq!(push_response.status_code, 200);
        let push_summary = serde_json::from_str::<Value>(&push_response.body).unwrap();
        assert_eq!(push_summary["acceptedCount"], json!(1));

        let device_b_cycle_push =
            serde_json::from_str::<Value>(&push_request("profile-a", "device-b", 120, Vec::new()))
                .unwrap();
        let cycle_body = json!({
            "pushRequest": device_b_cycle_push,
            "cursors": []
        })
        .to_string();
        let cycle_response = service.handle_http_request(BrowserSyncHttpRequest::new(
            "POST",
            browser_data::BROWSER_STORAGE_SYNC_TRANSPORT_CYCLE_PATH,
            sync_http_headers(
                &device_b.account_id,
                &device_b.device_id,
                &device_b.access_token,
            ),
            cycle_body,
        ));
        assert_eq!(cycle_response.status_code, 200);
        let cycle = serde_json::from_str::<Value>(&cycle_response.body).unwrap();
        assert_eq!(cycle["responseType"], json!("syncCycle"));
        assert_eq!(
            cycle["remoteBatch"]["records"][0]["recordType"],
            json!("historyPage")
        );
        assert_eq!(
            cycle["remoteBatch"]["records"][0]["payload"]["address"],
            json!("https://example.com/from-a")
        );

        let pull_response = service.handle_http_request(BrowserSyncHttpRequest::new(
            "POST",
            browser_data::BROWSER_STORAGE_SYNC_TRANSPORT_PULL_PATH,
            sync_http_headers(
                &device_b.account_id,
                &device_b.device_id,
                &device_b.access_token,
            ),
            json!({
                "version": 1,
                "storageSchemaVersion": 6,
                "requestType": "pull",
                "scope": "profile-a",
                "deviceId": "device-b",
                "generatedAtUnixSeconds": 130,
                "cursors": [{"collection":"historyPages","cursor":"0"}],
                "maxRecordsPerCollection": 100
            })
            .to_string(),
        ));
        assert_eq!(pull_response.status_code, 200);
        let pull = serde_json::from_str::<Value>(&pull_response.body).unwrap();
        assert_eq!(pull["records"][0]["recordType"], json!("historyPage"));

        let cursors_response = service.handle_http_request(BrowserSyncHttpRequest::new(
            "GET",
            browser_data::BROWSER_STORAGE_SYNC_TRANSPORT_CURSORS_PATH,
            sync_http_headers(
                &device_a.account_id,
                &device_a.device_id,
                &device_a.access_token,
            ),
            "",
        ));
        assert_eq!(cursors_response.status_code, 200);
        let cursors = serde_json::from_str::<Value>(&cursors_response.body).unwrap();
        assert_eq!(cursors[0]["collection"], json!("historyPages"));
    }

    #[test]
    fn http_facade_maps_auth_validation_and_routing_errors() {
        let mut service =
            BrowserSyncService::open_in_memory(BrowserSyncServiceConfig::default()).unwrap();
        service
            .register_device("profile-a", "device-a", "token-a-123", 100)
            .unwrap();

        let mut missing_token_headers = sync_http_headers("profile-a", "device-a", "token-a-123");
        missing_token_headers.retain(|(name, _)| !name.eq_ignore_ascii_case("Authorization"));
        let missing_token = service.handle_http_request(BrowserSyncHttpRequest::new(
            "POST",
            browser_data::BROWSER_STORAGE_SYNC_TRANSPORT_PUSH_PATH,
            missing_token_headers,
            "{}",
        ));
        assert_eq!(missing_token.status_code, 401);
        assert!(missing_token.body.contains("missingHeader"));

        let mut wrong_content_headers = sync_http_headers("profile-a", "device-a", "token-a-123");
        wrong_content_headers
            .iter_mut()
            .find(|(name, _)| name.eq_ignore_ascii_case("Content-Type"))
            .unwrap()
            .1 = "text/plain".to_string();
        let wrong_content_type = service.handle_http_request(BrowserSyncHttpRequest::new(
            "POST",
            browser_data::BROWSER_STORAGE_SYNC_TRANSPORT_PUSH_PATH,
            wrong_content_headers,
            "{}",
        ));
        assert_eq!(wrong_content_type.status_code, 415);
        assert!(wrong_content_type.body.contains("unsupportedMediaType"));

        let method_mismatch = service.handle_http_request(BrowserSyncHttpRequest::new(
            "GET",
            browser_data::BROWSER_STORAGE_SYNC_TRANSPORT_PUSH_PATH,
            sync_http_headers("profile-a", "device-a", "token-a-123"),
            "",
        ));
        assert_eq!(method_mismatch.status_code, 405);

        let not_found = service.handle_http_request(BrowserSyncHttpRequest::new(
            "GET",
            "/v1/browser-storage/sync/missing",
            sync_http_headers("profile-a", "device-a", "token-a-123"),
            "",
        ));
        assert_eq!(not_found.status_code, 404);

        let forbidden = service.handle_http_request(BrowserSyncHttpRequest::new(
            "POST",
            browser_data::BROWSER_STORAGE_SYNC_TRANSPORT_PULL_PATH,
            sync_http_headers("profile-a", "device-a", "token-a-123"),
            json!({
                "version": 1,
                "storageSchemaVersion": 6,
                "requestType": "pull",
                "scope": "profile-b",
                "deviceId": "device-a",
                "generatedAtUnixSeconds": 1,
                "cursors": []
            })
            .to_string(),
        ));
        assert_eq!(forbidden.status_code, 403);
        assert!(forbidden.body.contains("scope mismatch"));

        let unsupported_pull = service.handle_http_request(BrowserSyncHttpRequest::new(
            "POST",
            browser_data::BROWSER_STORAGE_SYNC_TRANSPORT_PULL_PATH,
            sync_http_headers("profile-a", "device-a", "token-a-123"),
            json!({
                "version": 1,
                "storageSchemaVersion": 5,
                "requestType": "pull",
                "scope": "profile-a",
                "deviceId": "device-a",
                "generatedAtUnixSeconds": 1,
                "cursors": []
            })
            .to_string(),
        ));
        assert_eq!(unsupported_pull.status_code, 426);
        assert!(
            unsupported_pull
                .body
                .contains(browser_data::BROWSER_STORAGE_SYNC_UNSUPPORTED_SCHEMA_ERROR_CODE)
        );
    }

    #[test]
    fn service_enforces_device_registration_scope_tokens_and_limits() {
        let mut service = BrowserSyncService::open_in_memory(BrowserSyncServiceConfig {
            max_push_records: 1,
            max_records_per_collection: 100,
        })
        .unwrap();
        let context = service
            .register_device(" profile-a ", " device-a ", " token-a-123 ", 100)
            .unwrap();
        let valid_push = push_request(
            "profile-a",
            "device-a",
            110,
            vec![history_record("device-a", "https://example.com/a", 110)],
        );
        assert!(
            service
                .merge_push_request_json(&context, &valid_push)
                .unwrap()
                .contains(r#""acceptedCount":1"#)
        );

        let bad_token = BrowserSyncRequestContext {
            account_id: "profile-a".to_string(),
            device_id: "device-a".to_string(),
            access_token: "wrong-token".to_string(),
        };
        assert!(matches!(
            service.merge_push_request_json(&bad_token, &valid_push),
            Err(BrowserSyncServerError::Unauthorized("token mismatch"))
        ));

        let unknown_device = BrowserSyncRequestContext {
            account_id: "profile-a".to_string(),
            device_id: "device-x".to_string(),
            access_token: "token-x-123".to_string(),
        };
        assert!(matches!(
            service.merge_push_request_json(
                &unknown_device,
                &push_request(
                    "profile-a",
                    "device-x",
                    111,
                    vec![history_record("device-x", "https://example.com/x", 111)]
                )
            ),
            Err(BrowserSyncServerError::Unauthorized("unknown device"))
        ));

        assert!(matches!(
            service.merge_push_request_json(
                &context,
                &push_request(
                    "profile-b",
                    "device-a",
                    112,
                    vec![history_record("device-a", "https://example.com/b", 112)]
                )
            ),
            Err(BrowserSyncServerError::Forbidden("scope mismatch"))
        ));

        assert!(matches!(
            service.merge_push_request_json(
                &context,
                &push_request(
                    "profile-a",
                    "device-a",
                    113,
                    vec![
                        history_record("device-a", "https://example.com/c", 113),
                        history_record("device-a", "https://example.com/d", 114),
                    ]
                )
            ),
            Err(BrowserSyncServerError::RequestLimit("push records"))
        ));

        let mut unsupported_push: Value = serde_json::from_str(&valid_push).unwrap();
        unsupported_push["storageSchemaVersion"] = json!(5);
        assert!(matches!(
            service.merge_push_request_json(&context, &unsupported_push.to_string()),
            Err(BrowserSyncServerError::UnsupportedStorageSchemaVersion {
                expected: BROWSER_STORAGE_SCHEMA_VERSION,
                actual: 5
            })
        ));

        let mut unsupported_push_batch: Value = serde_json::from_str(&valid_push).unwrap();
        unsupported_push_batch["pushBatch"]["storageSchemaVersion"] = json!(5);
        assert!(matches!(
            service.merge_push_request_json(&context, &unsupported_push_batch.to_string()),
            Err(BrowserSyncServerError::UnsupportedStorageSchemaVersion {
                expected: BROWSER_STORAGE_SCHEMA_VERSION,
                actual: 5
            })
        ));

        service
            .disable_device("profile-a", "device-a", 120)
            .unwrap();
        assert!(matches!(
            service.merge_push_request_json(&context, &valid_push),
            Err(BrowserSyncServerError::Unauthorized("disabled device"))
        ));
    }

    #[test]
    fn service_sync_cycle_uses_registered_identity_and_pull_limits() {
        let mut service = BrowserSyncService::open_in_memory(BrowserSyncServiceConfig {
            max_push_records: 10,
            max_records_per_collection: 1,
        })
        .unwrap();
        let device_a = service
            .register_device("profile-a", "device-a", "token-a-123", 100)
            .unwrap();
        let device_b = service
            .register_device("profile-a", "device-b", "token-b-123", 100)
            .unwrap();

        let device_a_push = push_request(
            "profile-a",
            "device-a",
            110,
            vec![
                history_record("device-a", "https://example.com/a", 110),
                history_record("device-a", "https://example.com/b", 111),
            ],
        );
        let summary = service
            .merge_push_request_json(&device_a, &device_a_push)
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&summary).unwrap()["acceptedCount"],
            json!(2)
        );

        let device_b_cycle = service
            .sync_cycle_json(
                &device_b,
                &push_request(
                    "profile-a",
                    "device-b",
                    120,
                    vec![bookmark_record(
                        "device-b",
                        "bookmark-1",
                        "https://example.com/bookmark",
                        120,
                    )],
                ),
                r#"[]"#,
            )
            .unwrap();
        let cycle = serde_json::from_str::<Value>(&device_b_cycle).unwrap();
        assert_eq!(cycle["ackBatch"]["records"][0]["syncRevision"], json!(3));

        let remote_records = cycle["remoteBatch"]["records"].as_array().unwrap();
        assert_eq!(remote_records.len(), 1);
        assert_eq!(remote_records[0]["recordType"], json!("historyPage"));
        assert_eq!(remote_records[0]["syncRevision"], json!(1));

        let next_pull = service
            .pull_remote_batch_json(
                &device_b,
                130,
                r#"[{"collection":"historyPages","cursor":"1"}]"#,
            )
            .unwrap();
        let next_batch = serde_json::from_str::<Value>(&next_pull).unwrap();
        assert_eq!(
            next_batch["records"][0]["payload"]["address"],
            json!("https://example.com/b")
        );

        let device_b_cursors = service.device_cursors_json(&device_b, 140).unwrap();
        assert!(device_b_cursors.contains(r#""collection":"bookmarks""#));
        assert!(device_b_cursors.contains(r#""cursor":"3""#));
    }
}
