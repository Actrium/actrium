import importlib.util
import re
import sqlite3
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
SCRIPT = Path(__file__).resolve().parents[1] / "migrate_schema_v2.py"
SPEC = importlib.util.spec_from_file_location("migrate_schema_v2", SCRIPT)
assert SPEC and SPEC.loader
MIGRATION = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MIGRATION)

V1_SCHEMA = """
CREATE TABLE mfr (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    public_key TEXT NOT NULL DEFAULT '',
    key_id TEXT NOT NULL DEFAULT '',
    descriptor_json TEXT NOT NULL DEFAULT '{}',
    contact TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at INTEGER NOT NULL,
    updated_at INTEGER,
    verified_at INTEGER,
    suspended_at INTEGER,
    revoked_at INTEGER,
    key_expires_at INTEGER
);
CREATE TABLE mfr_challenge (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    mfr_id INTEGER NOT NULL REFERENCES mfr(id),
    token TEXT NOT NULL,
    verify_url TEXT NOT NULL DEFAULT '',
    expires_at INTEGER NOT NULL,
    verified_at INTEGER,
    created_at INTEGER NOT NULL
);
CREATE TABLE mfr_package (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    mfr_id INTEGER NOT NULL REFERENCES mfr(id),
    manufacturer TEXT NOT NULL,
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    type_str TEXT NOT NULL,
    target TEXT NOT NULL,
    manifest TEXT NOT NULL,
    signature TEXT NOT NULL,
    signing_key_id TEXT,
    proto_files TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    published_at INTEGER NOT NULL,
    revoked_at INTEGER,
    UNIQUE(manufacturer, name, version, target)
);
CREATE INDEX idx_mfr_package_type ON mfr_package(type_str);
CREATE INDEX idx_mfr_package_mfr ON mfr_package(mfr_id, status);
CREATE TABLE mfr_key_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    mfr_id INTEGER NOT NULL REFERENCES mfr(id),
    key_id TEXT NOT NULL,
    public_key TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'retired',
    created_at INTEGER NOT NULL,
    retired_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX idx_mfr_key_history_lookup ON mfr_key_history(mfr_id, key_id);
CREATE TABLE mfr_publish_nonce (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    mfr_id INTEGER NOT NULL REFERENCES mfr(id),
    nonce BLOB NOT NULL UNIQUE,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE INDEX idx_mfr_publish_nonce_expires ON mfr_publish_nonce(expires_at);
CREATE TABLE ais_manufacturer_auth_nonce (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    manufacturer TEXT NOT NULL,
    key_id TEXT NOT NULL,
    nonce BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    UNIQUE(manufacturer, key_id, nonce)
);
CREATE INDEX idx_ais_manufacturer_auth_nonce_expires ON ais_manufacturer_auth_nonce(expires_at);
CREATE TABLE ais_renewal_token (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    actor_id TEXT NOT NULL,
    token_hash BLOB NOT NULL UNIQUE,
    expires_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    delivered_at INTEGER
);
CREATE INDEX idx_ais_renewal_token_actor ON ais_renewal_token(actor_id);
CREATE INDEX idx_ais_renewal_token_expires ON ais_renewal_token(expires_at);
CREATE TABLE ais_reissue_response_cache (
    nonce_hash BLOB PRIMARY KEY,
    response_ciphertext BLOB NOT NULL,
    expires_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_ais_reissue_response_cache_expires ON ais_reissue_response_cache(expires_at);
INSERT INTO mfr (id, name, key_id, created_at) VALUES (1, 'apps.example.com', 'actr-pub-0123456789abcdef', 1);
INSERT INTO mfr_challenge (mfr_id, token, expires_at, created_at) VALUES (1, 'tok', 2, 1);
INSERT INTO mfr_package
    (mfr_id, manufacturer, name, version, type_str, target, manifest, signature, published_at)
    VALUES (1, 'apps.example.com', 'echo', '1.0.0', 'apps.example.com/echo@1.0.0',
            'wasm32-wasip1', '', '', 1);
INSERT INTO mfr_key_history (mfr_id, key_id, public_key, created_at, retired_at)
    VALUES (1, 'actr-pub-fedcba9876543210', 'pk', 1, 2);
INSERT INTO ais_manufacturer_auth_nonce (manufacturer, key_id, nonce, created_at, expires_at)
    VALUES ('apps.example.com', 'actr-pub-0123456789abcdef', x'01', 1, 2);
INSERT INTO ais_renewal_token (actor_id, token_hash, expires_at, created_at)
    VALUES ('actor', x'02', 2, 1);
PRAGMA user_version = 1;
"""


def v2_table_columns(table: str) -> set[str]:
    """Read the v2 column names for ``table`` from the Actrix schema source."""
    source = (ROOT / "crates/loran-platform/src/storage/db.rs").read_text(encoding="utf-8")
    match = re.search(
        rf"CREATE TABLE IF NOT EXISTS {table} \((.*?)\n\s*\)\"", source, re.S
    )
    assert match, f"table {table} not found in db.rs"
    names = set()
    for line in match.group(1).splitlines():
        token = line.strip().split(" ", 1)[0]
        if token and not re.match(r"(?i)(UNIQUE|PRIMARY|FOREIGN)\b", token):
            names.add(token)
    return names


class MigrateSchemaV2Test(unittest.TestCase):
    def database(self, directory: str) -> sqlite3.Connection:
        db = sqlite3.connect(Path(directory) / "actrix.db")
        db.executescript(V1_SCHEMA)
        return db

    def test_renames_tables_columns_and_indexes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            db = self.database(directory)
            MIGRATION.migrate(db)

            self.assertEqual(db.execute("PRAGMA user_version").fetchone()[0], 2)
            tables = MIGRATION.tables(db)
            for old, new in MIGRATION.TABLE_RENAMES:
                self.assertNotIn(old, tables)
                self.assertIn(new, tables)
                self.assertEqual(MIGRATION.columns(db, new), v2_table_columns(new))

            indexes = {
                row[0]
                for row in db.execute("SELECT name FROM sqlite_master WHERE type = 'index'")
            }
            self.assertFalse(any("mfr" in name or "_ais_" in name for name in indexes))
            self.assertIn("idx_publisher_key_history_lookup", indexes)

            self.assertEqual(
                db.execute("SELECT publisher_id, publisher FROM publisher_package").fetchone(),
                (1, "apps.example.com"),
            )
            self.assertEqual(
                db.execute("SELECT publisher FROM issuer_publisher_auth_nonce").fetchone()[0],
                "apps.example.com",
            )
            self.assertEqual(db.execute("PRAGMA foreign_key_check").fetchall(), [])
            references = db.execute(
                "SELECT sql FROM sqlite_master WHERE name = 'publisher_challenge'"
            ).fetchone()[0]
            self.assertIn("REFERENCES \"publisher\"(id)", references)

    def test_rejects_other_versions(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            db = self.database(directory)
            db.execute("PRAGMA user_version = 0")
            with self.assertRaisesRegex(RuntimeError, "migrate_schema_v1"):
                MIGRATION.migrate(db)
            db.execute("PRAGMA user_version = 2")
            with self.assertRaisesRegex(RuntimeError, "already at schema version 2"):
                MIGRATION.migrate(db)


if __name__ == "__main__":
    unittest.main()
