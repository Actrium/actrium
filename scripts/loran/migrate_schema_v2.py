#!/usr/bin/env python3
"""Explicit, offline migration of the Actrix SQLite schema from v1 to v2.

Schema v2 retires the historical service names: the Publisher Registry tables
lose their `mfr` prefix, the Issuer tables lose their `ais` prefix, and the
publisher columns lose their `manufacturer`/`mfr_id` names.

Actrix must be stopped. Like the v1 tool, the whole run uses a single
connection: it writes a timestamped SQLite backup and then holds one
BEGIN EXCLUSIVE transaction across preflight and every statement. It never
prints Realm or service secrets.
"""

from __future__ import annotations

import argparse
import datetime as dt
import sqlite3
import sys
from pathlib import Path

SOURCE_VERSION = 1
TARGET_VERSION = 2

# (old table, new table)
TABLE_RENAMES = (
    ("mfr", "publisher"),
    ("mfr_challenge", "publisher_challenge"),
    ("mfr_package", "publisher_package"),
    ("mfr_key_history", "publisher_key_history"),
    ("mfr_publish_nonce", "publisher_publish_nonce"),
    ("ais_manufacturer_auth_nonce", "issuer_publisher_auth_nonce"),
    ("ais_renewal_token", "issuer_renewal_token"),
    ("ais_reissue_response_cache", "issuer_reissue_response_cache"),
)

# (new table, old column, new column), applied after the table renames.
COLUMN_RENAMES = (
    ("publisher_challenge", "mfr_id", "publisher_id"),
    ("publisher_package", "mfr_id", "publisher_id"),
    ("publisher_package", "manufacturer", "publisher"),
    ("publisher_key_history", "mfr_id", "publisher_id"),
    ("publisher_publish_nonce", "mfr_id", "publisher_id"),
    ("issuer_publisher_auth_nonce", "manufacturer", "publisher"),
)

# Indexes are recreated under their v2 names with the v2 definitions.
INDEX_RENAMES = (
    (
        "idx_mfr_package_type",
        "CREATE INDEX idx_publisher_package_type ON publisher_package(type_str)",
    ),
    (
        "idx_mfr_package_mfr",
        "CREATE INDEX idx_publisher_package_publisher "
        "ON publisher_package(publisher_id, status)",
    ),
    (
        "idx_mfr_key_history_lookup",
        "CREATE UNIQUE INDEX idx_publisher_key_history_lookup "
        "ON publisher_key_history(publisher_id, key_id)",
    ),
    (
        "idx_mfr_publish_nonce_expires",
        "CREATE INDEX idx_publisher_publish_nonce_expires "
        "ON publisher_publish_nonce(expires_at)",
    ),
    (
        "idx_ais_manufacturer_auth_nonce_expires",
        "CREATE INDEX idx_issuer_publisher_auth_nonce_expires "
        "ON issuer_publisher_auth_nonce(expires_at)",
    ),
    (
        "idx_ais_renewal_token_actor",
        "CREATE INDEX idx_issuer_renewal_token_actor ON issuer_renewal_token(actor_id)",
    ),
    (
        "idx_ais_renewal_token_expires",
        "CREATE INDEX idx_issuer_renewal_token_expires "
        "ON issuer_renewal_token(expires_at)",
    ),
    (
        "idx_ais_reissue_response_cache_expires",
        "CREATE INDEX idx_issuer_reissue_response_cache_expires "
        "ON issuer_reissue_response_cache(expires_at)",
    ),
)


def tables(db: sqlite3.Connection) -> set[str]:
    return {
        row[0]
        for row in db.execute("SELECT name FROM sqlite_master WHERE type = 'table'")
    }


def columns(db: sqlite3.Connection, table: str) -> set[str]:
    return {row[1] for row in db.execute(f"PRAGMA table_info({table})")}


def preflight(db: sqlite3.Connection) -> None:
    version = db.execute("PRAGMA user_version").fetchone()[0]
    if version == TARGET_VERSION:
        raise RuntimeError(f"database is already at schema version {TARGET_VERSION}")
    if version == 0:
        raise RuntimeError(
            "unversioned database: run migrate_schema_v1.py first, then this tool"
        )
    if version != SOURCE_VERSION:
        raise RuntimeError(f"unsupported source schema version {version}")

    present = tables(db)
    missing = [old for old, _ in TABLE_RENAMES if old not in present]
    if missing:
        raise RuntimeError(
            "schema v1 tables are missing: " + ", ".join(missing)
        )
    clashing = [new for _, new in TABLE_RENAMES if new in present]
    if clashing:
        raise RuntimeError(
            "schema v2 tables already exist: " + ", ".join(clashing)
        )


def apply_migration(db: sqlite3.Connection) -> None:
    """Apply the v2 renames inside an already-open transaction.

    The caller must have disabled foreign-key enforcement and opened the
    transaction; it owns COMMIT/ROLLBACK and re-enabling foreign keys. SQLite
    rewrites foreign-key references to renamed tables and columns itself.
    """
    for old_index, _ in INDEX_RENAMES:
        db.execute(f"DROP INDEX IF EXISTS {old_index}")
    for old, new in TABLE_RENAMES:
        db.execute(f"ALTER TABLE {old} RENAME TO {new}")
    for table, old, new in COLUMN_RENAMES:
        if old in columns(db, table):
            db.execute(f"ALTER TABLE {table} RENAME COLUMN {old} TO {new}")
    for _, create in INDEX_RENAMES:
        db.execute(create)
    db.execute(f"PRAGMA user_version = {TARGET_VERSION}")

    failures = db.execute("PRAGMA foreign_key_check").fetchall()
    if failures:
        raise RuntimeError(f"foreign-key check failed ({len(failures)} row(s))")


def migrate(db: sqlite3.Connection) -> None:
    """Self-contained migration for callers that hold no lock."""
    db.execute("PRAGMA foreign_keys = OFF")
    db.execute("BEGIN IMMEDIATE")
    try:
        preflight(db)
        apply_migration(db)
        db.commit()
    except Exception:
        db.rollback()
        raise
    finally:
        db.execute("PRAGMA foreign_keys = ON")


def write_backup(db: sqlite3.Connection, database: Path) -> Path:
    """Write a timestamped snapshot of ``database`` before any lock is taken."""
    timestamp = dt.datetime.now(dt.UTC).strftime("%Y%m%dT%H%M%SZ")
    backup = database.with_name(f"{database.name}.pre-v2.{timestamp}.bak")
    destination = sqlite3.connect(backup)
    try:
        db.backup(destination)
    finally:
        destination.close()
    return backup


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("database", type=Path, help="path to actrix.db")
    parser.add_argument(
        "--dry-run", action="store_true", help="perform preflight checks only"
    )
    args = parser.parse_args()
    database = args.database.resolve()
    if not database.is_file():
        parser.error(f"database does not exist: {database}")

    db = sqlite3.connect(f"file:{database}?mode=rw", uri=True)
    backup: Path | None = None
    try:
        db.execute("PRAGMA busy_timeout = 5000")
        db.execute("PRAGMA foreign_keys = OFF")
        if not args.dry_run:
            backup = write_backup(db, database)
        db.execute("BEGIN EXCLUSIVE")
        try:
            preflight(db)
            if args.dry_run:
                db.rollback()
                print("preflight passed; no changes made")
                return 0
            apply_migration(db)
            db.commit()
        except BaseException:
            db.rollback()
            if backup is not None:
                print(
                    f"migration failed; untouched backup is available at {backup}",
                    file=sys.stderr,
                )
            raise
        integrity = db.execute("PRAGMA integrity_check").fetchone()[0]
        if integrity != "ok":
            raise RuntimeError(f"integrity check failed: {integrity}")
    finally:
        db.execute("PRAGMA foreign_keys = ON")
        db.close()

    print(f"migration complete; backup: {backup}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
