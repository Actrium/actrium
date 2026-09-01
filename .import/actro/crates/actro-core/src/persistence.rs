pub const CORE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchemaMigration {
    pub version: u32,
    pub statements: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefaultTag {
    pub name: &'static str,
    pub color: &'static str,
    pub is_system: bool,
}

pub const DEFAULT_TAGS: &[DefaultTag] = &[
    DefaultTag {
        name: "App",
        color: "#007AFF",
        is_system: true,
    },
    DefaultTag {
        name: "Manufacturer",
        color: "#5856D6",
        is_system: true,
    },
    DefaultTag {
        name: "LastUsed",
        color: "#FF9500",
        is_system: true,
    },
    DefaultTag {
        name: "Frequent",
        color: "#FF2D55",
        is_system: true,
    },
    DefaultTag {
        name: "Work",
        color: "#34C759",
        is_system: false,
    },
    DefaultTag {
        name: "Personal",
        color: "#AF52DE",
        is_system: false,
    },
    DefaultTag {
        name: "Development",
        color: "#00C7BE",
        is_system: false,
    },
    DefaultTag {
        name: "Entertainment",
        color: "#FF3B30",
        is_system: false,
    },
];

pub const V1_STATEMENTS: &[&str] = &[
    "PRAGMA foreign_keys = ON",
    r#"
CREATE TABLE IF NOT EXISTS schema_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
)
"#,
    r#"
CREATE TABLE IF NOT EXISTS history (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    app_url TEXT,
    app_id TEXT,
    visit_time INTEGER NOT NULL
)
"#,
    "CREATE INDEX IF NOT EXISTS idx_history_time ON history(visit_time DESC)",
    r#"
CREATE TABLE IF NOT EXISTS bookmarks (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    app_url TEXT,
    app_id TEXT,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER,
    manufacturer TEXT,
    notes TEXT,
    favicon TEXT
)
"#,
    "CREATE INDEX IF NOT EXISTS idx_bookmarks_app_url ON bookmarks(app_url)",
    r#"
CREATE TABLE IF NOT EXISTS tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    color TEXT,
    is_system INTEGER DEFAULT 0
)
"#,
    r#"
CREATE TABLE IF NOT EXISTS bookmark_tags (
    bookmark_id TEXT NOT NULL,
    tag_id TEXT NOT NULL,
    PRIMARY KEY (bookmark_id, tag_id),
    FOREIGN KEY (bookmark_id) REFERENCES bookmarks(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
)
"#,
    "CREATE INDEX IF NOT EXISTS idx_bookmark_tags_tag ON bookmark_tags(tag_id)",
    r#"
CREATE TABLE IF NOT EXISTS apps (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    bundle_id TEXT NOT NULL UNIQUE,
    version TEXT,
    description TEXT,
    source_type TEXT NOT NULL,
    source_path TEXT NOT NULL,
    icon TEXT,
    installed_at INTEGER NOT NULL,
    last_used_at INTEGER,
    is_pinned INTEGER DEFAULT 0
)
"#,
    "CREATE INDEX IF NOT EXISTS idx_apps_bundle_id ON apps(bundle_id)",
    "CREATE INDEX IF NOT EXISTS idx_apps_source_type ON apps(source_type)",
    r#"
CREATE TABLE IF NOT EXISTS tab_sessions (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    icon TEXT,
    url TEXT,
    app_id TEXT,
    system_app_type TEXT,
    group_id TEXT,
    background_color TEXT,
    has_unread_message INTEGER DEFAULT 0,
    is_bookmarked INTEGER DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'unknown',
    sort_order INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL
)
"#,
    "CREATE INDEX IF NOT EXISTS idx_tab_sessions_sort ON tab_sessions(sort_order ASC)",
    r#"
CREATE TABLE IF NOT EXISTS vault_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
)
"#,
    r#"
CREATE TABLE IF NOT EXISTS vault_items (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    category TEXT NOT NULL DEFAULT 'login',
    fields_encrypted TEXT NOT NULL,
    notes TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
)
"#,
    "CREATE INDEX IF NOT EXISTS idx_vault_items_category ON vault_items(category)",
    "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('core_schema_version', '1')",
];

pub const MIGRATIONS: &[SchemaMigration] = &[SchemaMigration {
    version: CORE_SCHEMA_VERSION,
    statements: V1_STATEMENTS,
}];

pub mod query {
    pub mod history {
        pub const INSERT_VISIT: &str =
            "INSERT INTO history (id, title, app_url, app_id, visit_time) VALUES (?, ?, ?, ?, ?)";

        pub const SELECT_RECENT: &str = "SELECT * FROM history ORDER BY visit_time DESC LIMIT ?";

        pub const SELECT_ALL: &str = "SELECT * FROM history ORDER BY visit_time DESC";

        pub const SEARCH_BY_TITLE: &str =
            "SELECT * FROM history WHERE LOWER(title) LIKE ? ORDER BY visit_time DESC";

        pub const DELETE_BY_ID: &str = "DELETE FROM history WHERE id = ?";

        pub const DELETE_ALL: &str = "DELETE FROM history";
    }

    pub mod tags {
        pub const COUNT_ALL: &str = "SELECT COUNT(*) FROM tags";

        pub const SELECT_ALL: &str = "SELECT * FROM tags ORDER BY is_system DESC, name ASC";

        pub const SELECT_USER: &str = "SELECT * FROM tags WHERE is_system = 0 ORDER BY name ASC";

        pub const INSERT_TAG: &str =
            "INSERT INTO tags (id, name, color, is_system) VALUES (?, ?, ?, ?)";

        pub const UPDATE_TAG: &str = "UPDATE tags SET name = ?, color = ? WHERE id = ?";

        pub const DELETE_USER_TAG_BY_ID: &str = "DELETE FROM tags WHERE id = ? AND is_system = 0";

        pub const SELECT_BY_BOOKMARK_ID: &str = r#"
SELECT t.* FROM tags t
INNER JOIN bookmark_tags bt ON t.id = bt.tag_id
WHERE bt.bookmark_id = ?
"#;

        pub const DELETE_BOOKMARK_TAGS_BY_BOOKMARK_ID: &str =
            "DELETE FROM bookmark_tags WHERE bookmark_id = ?";

        pub const INSERT_BOOKMARK_TAG: &str =
            "INSERT INTO bookmark_tags (bookmark_id, tag_id) VALUES (?, ?)";

        pub const INSERT_OR_IGNORE_BOOKMARK_TAG: &str =
            "INSERT OR IGNORE INTO bookmark_tags (bookmark_id, tag_id) VALUES (?, ?)";

        pub const DELETE_BOOKMARK_TAG: &str =
            "DELETE FROM bookmark_tags WHERE bookmark_id = ? AND tag_id = ?";
    }

    pub mod bookmarks {
        pub const INSERT_BOOKMARK: &str = r#"
INSERT INTO bookmarks (id, title, app_url, app_id, created_at, last_used_at, manufacturer, notes, favicon)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
"#;

        pub const UPDATE_BOOKMARK: &str = r#"
UPDATE bookmarks SET title = ?, app_url = ?, app_id = ?, last_used_at = ?,
manufacturer = ?, notes = ?, favicon = ? WHERE id = ?
"#;

        pub const DELETE_BY_ID: &str = "DELETE FROM bookmarks WHERE id = ?";

        pub const SELECT_ALL: &str = "SELECT * FROM bookmarks ORDER BY title ASC";

        pub const SELECT_BY_TAG: &str = r#"
SELECT b.* FROM bookmarks b
INNER JOIN bookmark_tags bt ON b.id = bt.bookmark_id
WHERE bt.tag_id = ?
ORDER BY b.title ASC
"#;

        pub const SEARCH_BY_TITLE_OR_NOTES: &str = r#"
SELECT * FROM bookmarks
WHERE LOWER(title) LIKE ? OR LOWER(notes) LIKE ?
ORDER BY title ASC
"#;

        pub const COUNT_BY_APP_URL: &str = "SELECT COUNT(*) FROM bookmarks WHERE app_url = ?";

        pub const SELECT_BY_APP_URL: &str = "SELECT * FROM bookmarks WHERE app_url = ? LIMIT 1";

        pub const UPDATE_LAST_USED: &str = "UPDATE bookmarks SET last_used_at = ? WHERE id = ?";

        pub const DELETE_NULL_APP_URL: &str = "DELETE FROM bookmarks WHERE app_url IS NULL";

        pub const DELETE_DUPLICATES_BY_APP_URL: &str = r#"
DELETE FROM bookmarks WHERE id NOT IN (
    SELECT MIN(id) FROM bookmarks GROUP BY app_url
)
"#;

        pub const DELETE_BOOKMARK_TAGS: &str = "DELETE FROM bookmark_tags";

        pub const DELETE_ALL: &str = "DELETE FROM bookmarks";

        pub const VACUUM: &str = "VACUUM";
    }

    pub mod apps {
        pub const INSERT_OR_REPLACE_APP: &str = r#"
INSERT OR REPLACE INTO apps (id, name, bundle_id, version, description, source_type, source_path, icon, installed_at, last_used_at, is_pinned)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
"#;

        pub const UPDATE_APP: &str = r#"
UPDATE apps SET name = ?, version = ?, description = ?, source_path = ?, icon = ?, last_used_at = ?, is_pinned = ?
WHERE id = ?
"#;

        pub const DELETE_BY_ID: &str = "DELETE FROM apps WHERE id = ?";

        pub const DELETE_BY_BUNDLE_ID: &str = "DELETE FROM apps WHERE bundle_id = ?";

        pub const SELECT_ALL: &str = "SELECT * FROM apps ORDER BY name ASC";

        pub const SELECT_BY_SOURCE_TYPE: &str =
            "SELECT * FROM apps WHERE source_type = ? ORDER BY name ASC";

        pub const SELECT_PINNED: &str = "SELECT * FROM apps WHERE is_pinned = 1 ORDER BY name ASC";

        pub const SELECT_BY_BUNDLE_ID: &str = "SELECT * FROM apps WHERE bundle_id = ? LIMIT 1";

        pub const SELECT_BY_ID: &str = "SELECT * FROM apps WHERE id = ? LIMIT 1";

        pub const COUNT_BY_BUNDLE_ID: &str = "SELECT COUNT(*) FROM apps WHERE bundle_id = ?";

        pub const UPDATE_LAST_USED_BY_BUNDLE_ID: &str =
            "UPDATE apps SET last_used_at = ? WHERE bundle_id = ?";

        pub const TOGGLE_PINNED_BY_ID: &str =
            "UPDATE apps SET is_pinned = NOT is_pinned WHERE id = ?";

        pub const SEARCH_BY_NAME_DESCRIPTION_OR_BUNDLE_ID: &str = r#"
SELECT * FROM apps
WHERE LOWER(name) LIKE ? OR LOWER(description) LIKE ? OR LOWER(bundle_id) LIKE ?
ORDER BY name ASC
"#;

        pub const DELETE_ALL: &str = "DELETE FROM apps";

        pub const VACUUM: &str = "VACUUM";
    }
}

pub mod android {
    pub const DATABASE_NAME: &str = "actro_database";
    pub const DATABASE_VERSION: u32 = 1;

    pub mod active_list {
        pub const TABLE_NAME: &str = "active_list";

        pub mod column {
            pub const ID: &str = "id";
            pub const NAME: &str = "name";
            pub const ICON: &str = "icon";
            pub const SCREENSHOT: &str = "screenshot";
            pub const HAS_UNREAD: &str = "hasUnread";
            pub const URL: &str = "url";
            pub const IS_MARKED: &str = "isMarked";
            pub const STATUS: &str = "status";
            pub const SORT_ORDER: &str = "sortOrder";
        }

        pub mod query {
            pub const SELECT_ALL_ORDERED_BY_SORT_ORDER: &str =
                "SELECT * FROM active_list ORDER BY sortOrder ASC";
            pub const DELETE_ALL: &str = "DELETE FROM active_list";
            pub const COUNT_ALL: &str = "SELECT COUNT(*) FROM active_list";
        }
    }
}

pub fn schema_statements() -> Vec<&'static str> {
    MIGRATIONS
        .iter()
        .flat_map(|migration| migration.statements.iter().copied())
        .collect()
}

pub fn table_names() -> Vec<&'static str> {
    vec![
        "schema_meta",
        "history",
        "bookmarks",
        "tags",
        "bookmark_tags",
        "apps",
        "tab_sessions",
        "vault_meta",
        "vault_items",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_contains_current_apple_tables_and_android_tab_sessions() {
        let sql = schema_statements().join("\n");
        for table in [
            "history",
            "bookmarks",
            "tags",
            "bookmark_tags",
            "apps",
            "tab_sessions",
            "vault_meta",
            "vault_items",
        ] {
            assert!(
                sql.contains(&format!("CREATE TABLE IF NOT EXISTS {table}")),
                "missing table {table}"
            );
        }
    }

    #[test]
    fn default_tags_match_current_platform_seed_names() {
        let names: Vec<_> = DEFAULT_TAGS.iter().map(|tag| tag.name).collect();
        assert_eq!(
            names,
            vec![
                "App",
                "Manufacturer",
                "LastUsed",
                "Frequent",
                "Work",
                "Personal",
                "Development",
                "Entertainment"
            ]
        );
    }

    #[test]
    fn schema_version_is_recorded_by_latest_migration() {
        assert_eq!(CORE_SCHEMA_VERSION, 1);
        assert_eq!(MIGRATIONS.last().unwrap().version, CORE_SCHEMA_VERSION);
        assert!(schema_statements()
            .iter()
            .any(|statement| statement.contains("core_schema_version")));
    }

    #[test]
    fn repository_queries_match_core_schema_tables() {
        assert!(query::history::INSERT_VISIT.contains("INSERT INTO history"));
        assert!(query::history::SELECT_RECENT.contains("ORDER BY visit_time DESC LIMIT ?"));
        assert!(query::tags::SELECT_ALL.contains("ORDER BY is_system DESC, name ASC"));
        assert!(query::tags::SELECT_BY_BOOKMARK_ID.contains("bookmark_tags"));
        assert!(query::tags::INSERT_OR_IGNORE_BOOKMARK_TAG.contains("INSERT OR IGNORE"));
        assert!(query::bookmarks::INSERT_BOOKMARK.contains("INSERT INTO bookmarks"));
        assert!(query::bookmarks::SELECT_BY_TAG.contains("bookmark_tags"));
        assert!(query::bookmarks::DELETE_DUPLICATES_BY_APP_URL.contains("GROUP BY app_url"));
        assert!(query::apps::INSERT_OR_REPLACE_APP.contains("INSERT OR REPLACE INTO apps"));
        assert!(query::apps::SELECT_BY_SOURCE_TYPE.contains("source_type"));
        assert!(query::apps::SEARCH_BY_NAME_DESCRIPTION_OR_BUNDLE_ID.contains("bundle_id"));
    }

    #[test]
    fn android_active_room_contract_matches_existing_schema() {
        assert_eq!(android::DATABASE_NAME, "actro_database");
        assert_eq!(android::DATABASE_VERSION, 1);
        assert_eq!(android::active_list::TABLE_NAME, "active_list");
        assert_eq!(android::active_list::column::HAS_UNREAD, "hasUnread");
        assert_eq!(android::active_list::column::IS_MARKED, "isMarked");
        assert_eq!(android::active_list::column::SORT_ORDER, "sortOrder");
        assert_eq!(
            android::active_list::query::SELECT_ALL_ORDERED_BY_SORT_ORDER,
            "SELECT * FROM active_list ORDER BY sortOrder ASC"
        );
        assert_eq!(
            android::active_list::query::DELETE_ALL,
            "DELETE FROM active_list"
        );
        assert_eq!(
            android::active_list::query::COUNT_ALL,
            "SELECT COUNT(*) FROM active_list"
        );
    }
}
