import importlib.util
import sqlite3
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "migrate_schema_v1.py"
SPEC = importlib.util.spec_from_file_location("migrate_schema_v1", SCRIPT)
assert SPEC and SPEC.loader
MIGRATION = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MIGRATION)


def legacy_database(path: Path) -> sqlite3.Connection:
    db = sqlite3.connect(path)
    db.executescript(
        """
        CREATE TABLE realm (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            status TEXT NOT NULL,
            enabled INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            secret_current TEXT NOT NULL DEFAULT ''
        );
        CREATE TABLE mfr (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            public_key TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'pending'
        );
        CREATE TABLE mfr_package (
            id INTEGER PRIMARY KEY,
            mfr_id INTEGER NOT NULL,
            manufacturer TEXT NOT NULL,
            name TEXT NOT NULL,
            version TEXT NOT NULL
        );
        CREATE TABLE pending_registration (
            realm_id INTEGER NOT NULL,
            serial_number INTEGER NOT NULL,
            service_spec_blob BLOB,
            ws_address TEXT,
            created_at INTEGER NOT NULL,
            expires_at INTEGER
        );
        CREATE TABLE actoracl (
            rowid INTEGER PRIMARY KEY AUTOINCREMENT,
            realm_id INTEGER NOT NULL,
            source_realm_id INTEGER,
            from_type TEXT NOT NULL,
            to_type TEXT NOT NULL,
            access INTEGER NOT NULL
        );
        INSERT INTO realm
            (id, name, status, enabled, created_at, secret_current)
            VALUES (7, 'primary', 'Active', 1, 1, 'argon2-hash');
        INSERT INTO mfr
            (id, name, public_key, status)
            VALUES (1, 'apps.example.com', 'key', 'active');
        INSERT INTO actoracl
            (realm_id, source_realm_id, from_type, to_type, access)
            VALUES (7, NULL, 'a:x:1', 'b:y:1', 0);
        INSERT INTO actoracl
            (realm_id, source_realm_id, from_type, to_type, access)
            VALUES (7, NULL, 'a:x:1', 'b:y:1', 1);
        INSERT INTO pending_registration
            (realm_id, serial_number, service_spec_blob, ws_address, created_at, expires_at)
            VALUES (7, 1, NULL, '127.0.0.1:9999', 1, 1000);
        """
    )
    db.commit()
    return db


class SchemaV1MigrationTest(unittest.TestCase):
    def test_migrates_and_deduplicates_policy(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            db = legacy_database(Path(directory) / "actrix.db")
            MIGRATION.preflight(db, False)
            MIGRATION.migrate(db, False)

            self.assertEqual(db.execute("PRAGMA user_version").fetchone()[0], 1)
            self.assertIn("descriptor_json", MIGRATION.columns(db, "mfr"))
            self.assertIn("auth_mode", MIGRATION.columns(db, "issued_actor_identity"))
            self.assertIn("signing_key_id", MIGRATION.columns(db, "mfr_package"))
            self.assertEqual(
                db.execute("SELECT access FROM actoracl").fetchall(),
                [(1,)],
            )
            # pending_registration.expires_at must survive migration so the fresh-schema
            # CREATE INDEX ON pending_registration(expires_at) does not fail with
            # "no such column" on the next boot (the index itself is created by actrixd's
            # initialize_schema on first start, not by this migration script).
            self.assertIn("expires_at", MIGRATION.columns(db, "pending_registration"))
            row = db.execute(
                "SELECT expires_at FROM pending_registration WHERE realm_id = 7 AND serial_number = 1"
            ).fetchone()
            self.assertEqual(row[0], 1000)
            with self.assertRaises(sqlite3.IntegrityError):
                db.execute(
                    "INSERT INTO realm "
                    "(id, name, status, enabled, created_at, secret_current) "
                    "VALUES (8, 'other', 'Active', 1, 1, 'hash')"
                )
            db.close()

    def test_rejects_multiple_realms(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            db = legacy_database(Path(directory) / "actrix.db")
            db.execute(
                "INSERT INTO realm "
                "(id, name, status, enabled, created_at, secret_current) "
                "VALUES (8, 'other', 'Active', 1, 1, 'hash')"
            )
            db.commit()
            with self.assertRaisesRegex(RuntimeError, "one Realm"):
                MIGRATION.preflight(db, False)
            db.close()

    def test_rejects_implicit_legacy_publisher_mapping(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            db = legacy_database(Path(directory) / "actrix.db")
            db.execute("UPDATE mfr SET name = 'github-user'")
            db.commit()
            with self.assertRaisesRegex(RuntimeError, "HTTPS publisher"):
                MIGRATION.preflight(db, False)
            db.close()


if __name__ == "__main__":
    unittest.main()
