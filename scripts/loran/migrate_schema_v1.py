#!/usr/bin/env python3
"""Explicit, offline migration from the unversioned Actrix SQLite schema to v1.

Actrix must be stopped. The whole run uses a single connection: it writes a
timestamped read-only SQLite backup and then holds one BEGIN EXCLUSIVE
transaction across preflight and every DDL statement, so the safety gates
cannot be satisfied at check time and then invalidated by a concurrent writer
before the migration runs. It never prints Realm or service secrets.
"""

from __future__ import annotations

import argparse
import datetime as dt
import ipaddress
import sqlite3
import sys
from pathlib import Path

TARGET_VERSION = 1


def columns(db: sqlite3.Connection, table: str) -> set[str]:
    return {row[1] for row in db.execute(f"PRAGMA table_info({table})")}


def looks_like_domain(value: str) -> bool:
    value = value.strip().rstrip(".")
    if not value or any(char in value for char in "/@:"):
        return False
    try:
        ipaddress.ip_address(value)
        return False
    except ValueError:
        pass
    try:
        ascii_name = value.encode("idna").decode("ascii")
    except UnicodeError:
        return False
    return "." in ascii_name and len(ascii_name) <= 253


def preflight(db: sqlite3.Connection, drop_publishers: bool) -> None:
    version = db.execute("PRAGMA user_version").fetchone()[0]
    if version == TARGET_VERSION:
        raise RuntimeError("database is already at schema version 1")
    if version != 0:
        raise RuntimeError(f"unsupported source schema version {version}")

    realm_count = db.execute("SELECT COUNT(*) FROM realm").fetchone()[0]
    if realm_count > 1:
        raise RuntimeError(
            "v1 permits one Realm per Actrix instance; split or delete extra Realms first"
        )
    if realm_count == 1:
        secret = db.execute("SELECT secret_current FROM realm").fetchone()[0]
        if not secret or not secret.strip():
            raise RuntimeError(
                "the Realm has no registration secret; provision one before migration"
            )

    invalid_publishers = [
        name for (name,) in db.execute("SELECT name FROM mfr") if not looks_like_domain(name)
    ]
    if invalid_publishers and not drop_publishers:
        raise RuntimeError(
            "legacy GitHub-style manufacturer identities cannot become HTTPS publisher "
            "identities implicitly; re-run with --drop-publisher-data or re-provision "
            "them explicitly"
        )


def add_column(
    db: sqlite3.Connection, table: str, column: str, declaration: str
) -> None:
    if column not in columns(db, table):
        db.execute(f"ALTER TABLE {table} ADD COLUMN {column} {declaration}")


PUBLISHER_TABLES = (
    "ais_manufacturer_auth_nonce",
    "mfr_publish_nonce",
    "mfr_key_history",
    "mfr_package",
    "mfr_challenge",
    "mfr",
)


def confirm_publisher_drop(db: sqlite3.Connection, assume_yes: bool) -> None:
    """Report the exact rows that will be deleted and require confirmation.

    --drop-publisher-data performs an unconditional DELETE FROM on every
    publisher table; it does NOT filter by domain mapping. Print the row counts
    so the operator sees the full blast radius before any data is removed.
    """
    counts = {
        table: db.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
        for table in PUBLISHER_TABLES
    }
    total = sum(counts.values())
    print("WARNING: --drop-publisher-data deletes ALL publisher data.")
    print("The following tables will be emptied completely (full DELETE FROM):")
    for table in PUBLISHER_TABLES:
        print(f"  {table}: {counts[table]} row(s)")
    print(f"Total rows to delete: {total}")
    if total == 0:
        return
    if assume_yes:
        print("Proceeding without prompt (--assume-yes).")
        return
    answer = input("Type 'yes' to permanently delete this data: ").strip()
    if answer != "yes":
        raise RuntimeError("aborted by operator; no data was deleted")


def apply_migration(
    db: sqlite3.Connection, drop_publishers: bool, assume_yes: bool
) -> None:
    """Apply the v1 DDL inside an already-open transaction.

    The caller must have disabled foreign-key enforcement and opened the
    transaction: SQLite ignores ``PRAGMA foreign_keys`` while a transaction is
    active and a nested ``BEGIN`` is an error. The caller owns COMMIT/ROLLBACK
    and re-enabling foreign keys. Runs the foreign-key check before returning so
    the caller can commit knowing referential integrity holds.
    """
    if drop_publishers:
        confirm_publisher_drop(db, assume_yes)
        for table in PUBLISHER_TABLES:
            db.execute(f"DELETE FROM {table}")

    add_column(db, "mfr", "key_id", "TEXT NOT NULL DEFAULT ''")
    add_column(db, "mfr", "descriptor_json", "TEXT NOT NULL DEFAULT '{}'")
    add_column(
        db,
        "mfr_package",
        "target",
        "TEXT NOT NULL DEFAULT 'wasm32-wasip1'",
    )
    add_column(db, "mfr_package", "proto_files", "TEXT")
    add_column(db, "mfr_package", "signing_key_id", "TEXT")

    pending_columns = columns(db, "pending_registration")
    ws_select = "ws_address" if "ws_address" in pending_columns else "NULL"
    # The fresh schema's pending_registration carries an expires_at column (signaling
    # reads `expires_at IS NULL OR expires_at > unixepoch()` and db.rs creates an index
    # on it). The legacy table gained the column via ALTER, so migrate the values; if a
    # legacy DB somehow lacks it, NULL preserves the "never expires" semantics.
    expires_select = "expires_at" if "expires_at" in pending_columns else "NULL"
    db.execute(
        """
        CREATE TABLE pending_registration_v1 (
            realm_id INTEGER NOT NULL,
            serial_number INTEGER NOT NULL,
            service_spec_blob BLOB,
            ws_address TEXT,
            created_at INTEGER NOT NULL,
            expires_at INTEGER,
            PRIMARY KEY(realm_id, serial_number),
            FOREIGN KEY(realm_id) REFERENCES realm(id) ON DELETE CASCADE
        )
        """
    )
    db.execute(
        f"""
        INSERT INTO pending_registration_v1
            (realm_id, serial_number, service_spec_blob, ws_address, created_at, expires_at)
        SELECT realm_id, serial_number, service_spec_blob, {ws_select}, created_at, {expires_select}
        FROM pending_registration
        """
    )
    db.execute("DROP TABLE pending_registration")
    db.execute(
        "ALTER TABLE pending_registration_v1 RENAME TO pending_registration"
    )
    db.execute(
        """
        CREATE TABLE issued_actor_identity (
            realm_id INTEGER NOT NULL,
            serial_number INTEGER NOT NULL,
            actor_type TEXT NOT NULL,
            target TEXT,
            auth_mode INTEGER,
            issued_at INTEGER NOT NULL,
            PRIMARY KEY(realm_id, serial_number),
            FOREIGN KEY(realm_id) REFERENCES realm(id) ON DELETE CASCADE
        )
        """
    )

    db.execute(
        """
        CREATE TABLE actoracl_v1 (
            rowid INTEGER PRIMARY KEY AUTOINCREMENT,
            realm_id INTEGER NOT NULL,
            source_realm_id INTEGER,
            from_type TEXT NOT NULL,
            to_type TEXT NOT NULL,
            access INTEGER NOT NULL CHECK(access IN (0, 1)),
            FOREIGN KEY(realm_id) REFERENCES realm(id) ON DELETE CASCADE
        )
        """
    )
    db.execute(
        """
        INSERT INTO actoracl_v1
            (realm_id, source_realm_id, from_type, to_type, access)
        SELECT a.realm_id, a.source_realm_id, a.from_type, a.to_type, a.access
        FROM actoracl AS a
        WHERE a.rowid = (
            SELECT MAX(b.rowid)
            FROM actoracl AS b
            WHERE b.realm_id = a.realm_id
              AND COALESCE(b.source_realm_id, -1) =
                  COALESCE(a.source_realm_id, -1)
              AND b.from_type = a.from_type
              AND b.to_type = a.to_type
        )
        """
    )
    db.execute("DROP TABLE actoracl")
    db.execute("ALTER TABLE actoracl_v1 RENAME TO actoracl")

    db.execute("CREATE UNIQUE INDEX idx_realm_singleton ON realm((1))")
    db.execute("DROP INDEX IF EXISTS idx_realm_name")
    db.execute("CREATE UNIQUE INDEX idx_realm_name ON realm(name)")
    db.execute(
        "CREATE INDEX idx_actoracl_realm_id ON actoracl(realm_id)"
    )
    db.execute(
        "CREATE INDEX idx_actoracl_lookup "
        "ON actoracl(realm_id, source_realm_id, from_type, to_type)"
    )
    db.execute(
        "CREATE UNIQUE INDEX idx_actoracl_policy ON actoracl("
        "realm_id, COALESCE(source_realm_id, -1), from_type, to_type)"
    )
    db.execute(f"PRAGMA user_version = {TARGET_VERSION}")

    failures = db.execute("PRAGMA foreign_key_check").fetchall()
    if failures:
        raise RuntimeError(f"foreign-key check failed ({len(failures)} row(s))")


def migrate(
    db: sqlite3.Connection, drop_publishers: bool, assume_yes: bool = False
) -> None:
    """Self-contained migration for callers that hold no lock: owns the whole
    transaction and the foreign-key toggle.

    The CLI entry point does not use this; it runs :func:`preflight` and
    :func:`apply_migration` together under one ``BEGIN EXCLUSIVE`` transaction
    (see :func:`main`). This wrapper opens its own ``BEGIN IMMEDIATE`` so direct
    and unit-test callers still get atomic behaviour.
    """
    db.execute("PRAGMA foreign_keys = OFF")
    db.execute("BEGIN IMMEDIATE")
    try:
        apply_migration(db, drop_publishers, assume_yes)
        db.commit()
    except Exception:
        db.rollback()
        raise
    finally:
        db.execute("PRAGMA foreign_keys = ON")


def write_backup(db: sqlite3.Connection, database: Path) -> Path:
    """Write a timestamped read-only snapshot of ``database`` through ``db``.

    Must be called while ``db`` is in autocommit mode. The online backup API
    deadlocks when the source connection already holds a write lock, so the
    snapshot is taken before BEGIN EXCLUSIVE. Preflight still runs inside the
    lock, so the safety gates stay race-free; this snapshot is only a rollback
    safety net.
    """
    timestamp = dt.datetime.now(dt.UTC).strftime("%Y%m%dT%H%M%SZ")
    backup = database.with_name(f"{database.name}.pre-v1.{timestamp}.bak")
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
        "--drop-publisher-data",
        action="store_true",
        help=(
            "delete ALL publisher data: unconditionally empties every publisher "
            "table (mfr, mfr_package, mfr_key_history, mfr_challenge, "
            "mfr_publish_nonce, ais_manufacturer_auth_nonce). This is a full wipe, "
            "NOT a filtered removal of entries lacking a domain mapping. Row counts "
            "are printed and confirmation is required before deletion."
        ),
    )
    parser.add_argument(
        "--assume-yes",
        action="store_true",
        help="skip the interactive confirmation prompt for --drop-publisher-data",
    )
    parser.add_argument(
        "--dry-run", action="store_true", help="perform preflight checks only"
    )
    args = parser.parse_args()
    database = args.database.resolve()
    if not database.is_file():
        parser.error(f"database does not exist: {database}")

    # One connection for the entire run; explicitly closed in the finally block
    # (the context-manager form only commits, it never closes the connection).
    db = sqlite3.connect(f"file:{database}?mode=rw", uri=True)
    backup: Path | None = None
    try:
        db.execute("PRAGMA busy_timeout = 5000")
        # foreign_keys must be toggled before the transaction opens; SQLite
        # silently ignores the pragma while a transaction is active.
        db.execute("PRAGMA foreign_keys = OFF")
        # Snapshot before locking: db.backup() deadlocks once the source
        # connection holds a write lock. A dry run makes no changes, so it needs
        # no snapshot.
        if not args.dry_run:
            backup = write_backup(db, database)
        # A single exclusive transaction spans preflight and every DDL statement,
        # so a concurrent writer cannot invalidate a safety gate between the check
        # and the migration (the preflight TOCTOU window).
        db.execute("BEGIN EXCLUSIVE")
        try:
            preflight(db, args.drop_publisher_data)
            if args.dry_run:
                db.rollback()
                print("preflight passed; no changes made")
                return 0
            apply_migration(db, args.drop_publisher_data, args.assume_yes)
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
