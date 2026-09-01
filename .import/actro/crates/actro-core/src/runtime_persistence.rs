pub const RUNTIME_DB_SCHEMA_VERSION: u32 = 4;
pub const RUNTIME_DATABASE_NAME: &str = "AppDatabase.db";

pub mod table {
    pub const APPS: &str = "apps";
    pub const AUTH: &str = "auth";
    pub const BOOKMARK_CATEGORIES: &str = "bookmark_categories";
    pub const BOOKMARK_ITEMS: &str = "bookmark_items";
    pub const APP_CHAT_HISTORY: &str = "app_chat_history";
}

pub mod column {
    pub mod apps {
        pub const APP_STATUS: &str = "app_status";
        pub const INSTANCE_NAME: &str = "instance_name";
    }
}

pub mod app_status {
    pub const ALIVE: &str = "alive";
    pub const DEAD: &str = "dead";
}

pub fn default_app_status() -> &'static str {
    app_status::ALIVE
}

pub fn normalize_app_status(value: &str) -> Option<&'static str> {
    match value {
        app_status::ALIVE => Some(app_status::ALIVE),
        app_status::DEAD => Some(app_status::DEAD),
        _ => None,
    }
}

pub mod message_type {
    pub const TEXT: &str = "text";
    pub const VOICE: &str = "voice";
}

pub mod message_progress {
    pub const PENDING: u8 = 0;
    pub const PROCESSING: u8 = 1;
    pub const COMPLETED: u8 = 2;
    pub const FAILED: u8 = 3;
    pub const CANCELED: u8 = 4;
    pub const OVERDUE: u8 = 5;
}

pub const BASE_SCHEMA_STATEMENTS: &[&str] = &[
    r#"
CREATE TABLE IF NOT EXISTS bookmark_categories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    sort_order INTEGER DEFAULT 0
)
"#,
    r#"
CREATE TABLE IF NOT EXISTS apps (
    app_id TEXT PRIMARY KEY,
    app_status TEXT DEFAULT 'alive',
    app_name TEXT,
    instance_name TEXT,
    full_description TEXT,
    full_title TEXT,
    logo TEXT,
    url TEXT,
    details TEXT,
    last_opened INTEGER
)
"#,
    r#"
CREATE TABLE IF NOT EXISTS bookmark_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    category_id INTEGER,
    app_id TEXT NOT NULL,
    created_at INTEGER DEFAULT (strftime('%s','now')),
    sort_order INTEGER DEFAULT 0,
    FOREIGN KEY (category_id) REFERENCES bookmark_categories (id) ON DELETE CASCADE,
    FOREIGN KEY (app_id) REFERENCES apps (app_id) ON DELETE CASCADE
)
"#,
    r#"
CREATE UNIQUE INDEX IF NOT EXISTS idx_bookmark_items_app_id_unique
ON bookmark_items (app_id)
"#,
    r#"
CREATE TABLE IF NOT EXISTS app_chat_history (
    message_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    app_id TEXT NOT NULL,
    tab_id TEXT NOT NULL,
    question_type TEXT DEFAULT 'text',
    question_time INTEGER,
    question_content TEXT,
    question_voice_payload TEXT,
    answer_type TEXT DEFAULT 'text',
    answer_time INTEGER,
    answer_content TEXT,
    answer_voice_payload TEXT,
    attachments TEXT,
    is_preset INTEGER DEFAULT 0,
    is_edited INTEGER DEFAULT 0,
    is_read INTEGER DEFAULT 0,
    progress INTEGER DEFAULT 0,
    FOREIGN KEY (app_id) REFERENCES apps (app_id) ON DELETE CASCADE
)
"#,
    r#"
CREATE TABLE IF NOT EXISTS auth (
    env TEXT PRIMARY KEY,
    token TEXT,
    updated_at INTEGER
)
"#,
    r#"
CREATE TRIGGER IF NOT EXISTS update_auth_updated_at
AFTER UPDATE ON auth
BEGIN
    UPDATE auth SET updated_at = (strftime('%s','now') * 1000) WHERE env = OLD.env;
END;
"#,
    r#"
CREATE INDEX IF NOT EXISTS idx_chat_history_tab_read
ON app_chat_history (tab_id, is_read)
"#,
];

pub const ADD_APPS_INSTANCE_NAME_COLUMN_SQL: &str =
    "ALTER TABLE apps ADD COLUMN instance_name TEXT";

pub const REMOVE_DUPLICATE_BOOKMARK_ITEMS_BY_APP_ID_SQL: &str = r#"
DELETE FROM bookmark_items
WHERE id NOT IN (
    SELECT MIN(id)
    FROM bookmark_items
    GROUP BY app_id
)
"#;

pub const CREATE_UNIQUE_BOOKMARK_ITEMS_APP_ID_INDEX_SQL: &str = r#"
CREATE UNIQUE INDEX IF NOT EXISTS idx_bookmark_items_app_id_unique
ON bookmark_items (app_id)
"#;

pub const ADD_APPS_APP_STATUS_COLUMN_SQL: &str =
    "ALTER TABLE apps ADD COLUMN app_status TEXT DEFAULT 'alive'";

pub mod query {
    pub mod apps {
        pub const UPSERT: &str = r#"
INSERT OR REPLACE INTO apps
(app_id, app_status, app_name, instance_name, full_description, full_title, logo, url, details, last_opened)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
"#;

        pub const SELECT_DETAILS_BY_APP_ID: &str =
            "SELECT details, app_status FROM apps WHERE app_id = ?";

        pub const UPDATE_LAST_OPENED: &str = "UPDATE apps SET last_opened = ? WHERE app_id = ?";

        pub const UPDATE_STATUS_AND_DETAILS: &str = r#"
UPDATE apps
SET app_status = ?, details = ?
WHERE app_id = ?
"#;
    }

    pub mod auth {
        pub const UPSERT_TOKEN: &str = r#"
INSERT OR REPLACE INTO auth (env, token, updated_at)
VALUES (?, ?, ?)
"#;

        pub const SELECT_TOKEN_BY_ENV: &str = "SELECT token FROM auth WHERE env = ?";
        pub const DELETE_TOKEN_BY_ENV: &str = "DELETE FROM auth WHERE env = ?";
    }

    pub mod messages {
        pub const SELECT_BY_TAB_ID: &str = r#"
SELECT *
FROM app_chat_history
WHERE tab_id = ?
ORDER BY question_time DESC
LIMIT ? OFFSET ?
"#;

        pub const COUNT_BY_TAB_ID_AND_APP_ID: &str = r#"
SELECT COUNT(*) as count
FROM app_chat_history
WHERE tab_id = ? AND app_id = ?
"#;

        pub const UPSERT: &str = r#"
INSERT OR REPLACE INTO app_chat_history
(message_id, app_id, tab_id, session_id, question_type, question_time, question_content, question_voice_payload, answer_type, answer_time, answer_content, answer_voice_payload, attachments, is_preset, is_edited, is_read, progress)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
"#;

        pub const DELETE_BY_MESSAGE_ID: &str = "DELETE FROM app_chat_history WHERE message_id = ?";

        pub const DELETE_BY_TAB_ID: &str = "DELETE FROM app_chat_history WHERE tab_id = ?";

        pub const MARK_TAB_AS_READ: &str =
            "UPDATE app_chat_history SET is_read = 1 WHERE tab_id = ? AND is_read = 0";

        pub const COUNT_UNREAD_BY_TAB_ID: &str = r#"
SELECT COUNT(*) as count
FROM app_chat_history
WHERE tab_id = ? AND is_read = 0
"#;

        pub const COUNT_UNREAD_BY_ALL_TABS: &str = r#"
SELECT tab_id, COUNT(*) as count
FROM app_chat_history
WHERE is_read = 0
GROUP BY tab_id
"#;

        pub const EXISTS_UNREAD_BY_TAB_ID: &str = r#"
SELECT EXISTS(
    SELECT 1
    FROM app_chat_history
    WHERE tab_id = ? AND is_read = 0
    LIMIT 1
) as exists
"#;
    }

    pub mod bookmarks {
        pub const SELECT_CATEGORIES: &str =
            "SELECT id, title, sort_order FROM bookmark_categories ORDER BY sort_order ASC";

        pub const SELECT_ITEMS_WITH_APP_INFO: &str = r#"
SELECT
    bi.id,
    bi.category_id,
    bi.app_id,
    bi.created_at,
    bi.sort_order,
    a.app_name,
    a.instance_name,
    a.app_status,
    a.logo,
    a.url,
    (SELECT question_content FROM app_chat_history WHERE app_id = bi.app_id ORDER BY question_time DESC, message_id DESC LIMIT 1) as last_question,
    (SELECT answer_content FROM app_chat_history WHERE app_id = bi.app_id ORDER BY question_time DESC, message_id DESC LIMIT 1) as last_answer
FROM bookmark_items bi
LEFT JOIN apps a ON bi.app_id = a.app_id
ORDER BY bi.sort_order ASC
"#;

        pub const INSERT_CATEGORY: &str =
            "INSERT INTO bookmark_categories (title, sort_order) VALUES (?, ?)";

        pub const DELETE_CATEGORY_BY_ID: &str = "DELETE FROM bookmark_categories WHERE id = ?";

        pub const INSERT_BOOKMARK: &str =
            "INSERT OR IGNORE INTO bookmark_items (category_id, app_id, sort_order) VALUES (?, ?, ?)";

        pub const DELETE_BOOKMARK_BY_ID: &str = "DELETE FROM bookmark_items WHERE id = ?";

        pub const DELETE_BOOKMARK_BY_APP_ID: &str = "DELETE FROM bookmark_items WHERE app_id = ?";

        pub const SELECT_BOOKMARK_BY_APP_ID: &str = r#"
SELECT
    bi.id,
    bi.category_id,
    bi.app_id,
    bi.created_at,
    bi.sort_order,
    a.app_name,
    a.instance_name,
    a.app_status,
    a.logo,
    a.url,
    (SELECT question_content FROM app_chat_history WHERE app_id = bi.app_id ORDER BY question_time DESC, message_id DESC LIMIT 1) as last_question,
    (SELECT answer_content FROM app_chat_history WHERE app_id = bi.app_id ORDER BY question_time DESC, message_id DESC LIMIT 1) as last_answer
FROM bookmark_items bi
LEFT JOIN apps a ON bi.app_id = a.app_id
WHERE bi.app_id = ?
ORDER BY bi.sort_order ASC, bi.id ASC
LIMIT 1
"#;
    }
}

pub fn base_schema_statements() -> Vec<&'static str> {
    BASE_SCHEMA_STATEMENTS.to_vec()
}

pub fn table_names() -> Vec<&'static str> {
    vec![
        table::BOOKMARK_CATEGORIES,
        table::APPS,
        table::BOOKMARK_ITEMS,
        table::APP_CHAT_HISTORY,
        table::AUTH,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_schema_contains_current_tables_and_indexes() {
        let sql = base_schema_statements().join("\n");
        for table in table_names() {
            assert!(
                sql.contains(&format!("CREATE TABLE IF NOT EXISTS {table}")),
                "missing runtime table {table}"
            );
        }
        assert!(sql.contains("idx_bookmark_items_app_id_unique"));
        assert!(sql.contains("idx_chat_history_tab_read"));
        assert!(sql.contains("update_auth_updated_at"));
    }

    #[test]
    fn runtime_schema_version_matches_latest_migration() {
        assert_eq!(RUNTIME_DB_SCHEMA_VERSION, 4);
        assert_eq!(RUNTIME_DATABASE_NAME, "AppDatabase.db");
        assert!(ADD_APPS_INSTANCE_NAME_COLUMN_SQL.contains(column::apps::INSTANCE_NAME));
        assert!(ADD_APPS_APP_STATUS_COLUMN_SQL.contains(column::apps::APP_STATUS));
    }

    #[test]
    fn runtime_query_contracts_cover_repositories() {
        assert!(query::apps::UPSERT.contains(table::APPS));
        assert!(query::auth::UPSERT_TOKEN.contains(table::AUTH));
        assert!(query::messages::UPSERT.contains(table::APP_CHAT_HISTORY));
        assert!(query::bookmarks::SELECT_ITEMS_WITH_APP_INFO.contains("last_question"));
        assert!(query::bookmarks::SELECT_BOOKMARK_BY_APP_ID.contains("LIMIT 1"));
    }

    #[test]
    fn runtime_state_values_are_stable() {
        assert_eq!(app_status::ALIVE, "alive");
        assert_eq!(app_status::DEAD, "dead");
        assert_eq!(default_app_status(), app_status::ALIVE);
        assert_eq!(normalize_app_status("alive"), Some(app_status::ALIVE));
        assert_eq!(normalize_app_status("dead"), Some(app_status::DEAD));
        assert_eq!(normalize_app_status("Dead"), None);
        assert_eq!(normalize_app_status(" dead "), None);
        assert_eq!(message_type::TEXT, "text");
        assert_eq!(message_type::VOICE, "voice");
        assert_eq!(message_progress::PENDING, 0);
        assert_eq!(message_progress::COMPLETED, 2);
        assert_eq!(message_progress::OVERDUE, 5);
    }
}
