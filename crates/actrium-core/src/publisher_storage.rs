use std::collections::BTreeMap;

use actr_protocol::{KeyPinOutcome, KeyPins, PackageSigningKey, PublisherKeys, PublisherProfile};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD as BASE64, URL_SAFE_NO_PAD},
};
use chrono::Utc;
use rand::RngCore;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};

use crate::guest_local_storage::{
    MAX_KEY_BYTES, MAX_ORIGIN_BYTES, MAX_ORIGIN_ENTRIES, MAX_VALUE_BYTES,
};
use crate::{
    GuestLocalStorage, GuestLocalStorageError as Error, PublisherPackageKeyInfo,
    PublisherProfileAcceptance, PublisherProfileInfo,
};

pub(crate) fn initialize(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS guest_publishers (
             domain TEXT PRIMARY KEY, identity TEXT NOT NULL UNIQUE,
             pins TEXT NOT NULL, profile TEXT NOT NULL, accepted_at TEXT NOT NULL
         ) WITHOUT ROWID;
         CREATE TABLE IF NOT EXISTS guest_publisher_keys (
             domain TEXT NOT NULL, public_key TEXT NOT NULL,
             PRIMARY KEY (domain, public_key)
         ) WITHOUT ROWID;
         CREATE TABLE IF NOT EXISTS guest_storage_programs (
             publisher_identity TEXT NOT NULL, program TEXT NOT NULL, origin TEXT NOT NULL UNIQUE,
             PRIMARY KEY(publisher_identity, program)
         ) WITHOUT ROWID;
         CREATE TABLE IF NOT EXISTS guest_storage_aliases (
             alias TEXT PRIMARY KEY, origin TEXT NOT NULL,
             publisher_identity TEXT NOT NULL, program TEXT NOT NULL
         ) WITHOUT ROWID;",
    )
}

struct SavedPublisher {
    identity: String,
    pins: KeyPins,
    profile: PublisherProfile,
}

fn invalid(error: impl std::fmt::Display) -> Error {
    Error::InvalidPublisher(error.to_string())
}

fn canonical_domain(domain: &str) -> Result<(), Error> {
    let expected = format!("https://{domain}/");
    let parsed = url::Url::parse(&expected).map_err(invalid)?;
    if domain.is_empty() || parsed.as_str() != expected || parsed.host_str() != Some(domain) {
        return Err(invalid("Publisher domain is not canonical"));
    }
    Ok(())
}

fn load(connection: &Connection, domain: &str) -> Result<Option<SavedPublisher>, Error> {
    let row = connection
        .query_row(
            "SELECT identity, pins, profile, accepted_at FROM guest_publishers WHERE domain = ?1",
            [domain],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;
    row.map(|(identity, pins, profile, accepted_at)| {
        restore(domain, identity, pins, profile, accepted_at)
    })
    .transpose()
}

fn restore(
    domain: &str,
    identity: String,
    pins: String,
    profile: String,
    accepted_at: String,
) -> Result<SavedPublisher, Error> {
    if URL_SAFE_NO_PAD
        .decode(&identity)
        .ok()
        .is_none_or(|bytes| bytes.len() != 16)
    {
        return Err(Error::DatabaseFailure(
            "Stored Publisher identity is invalid".into(),
        ));
    }
    let corrupt =
        |error| Error::DatabaseFailure(format!("Stored Publisher trust is invalid: {error}"));
    let saved = SavedPublisher {
        identity,
        pins: serde_json::from_str(&pins).map_err(corrupt)?,
        profile: serde_json::from_str(&profile).map_err(corrupt)?,
    };
    let time = chrono::DateTime::parse_from_rfc3339(&accepted_at).map_err(|error| {
        Error::DatabaseFailure(format!(
            "Stored Publisher acceptance time is invalid: {error}"
        ))
    })?;
    let (_, outcome) = saved
        .profile
        .accept(domain, time.with_timezone(&Utc), Some(&saved.pins))
        .map_err(|error| {
            Error::DatabaseFailure(format!("Stored Publisher trust is invalid: {error}"))
        })?;
    if !matches!(outcome, KeyPinOutcome::Accepted(_)) {
        return Err(Error::DatabaseFailure(
            "Stored Publisher history is incomplete".into(),
        ));
    }
    Ok(saved)
}

impl GuestLocalStorage {
    /// The Host supplies a document fetched from the bound Publisher over HTTPS.
    pub fn accept_publisher_profile(
        &self,
        body: String,
        domain: String,
        previous_pins_json: Option<String>,
    ) -> Result<PublisherProfileAcceptance, Error> {
        canonical_domain(&domain)?;
        if body.len() > 2 * 1024 * 1024 {
            return Err(invalid("Publisher document exceeds 2 MiB"));
        }
        let document = actr_protocol::signed_json::parse(body.as_bytes()).map_err(invalid)?;
        let descriptor: actr_protocol::AuthorityDescriptor =
            serde_json::from_value(document).map_err(invalid)?;
        let uri = actr_protocol::uri::ActrUri::from_domain(&domain).map_err(invalid)?;
        descriptor.validate_for(&uri).map_err(invalid)?;
        let profile = descriptor
            .publisher
            .ok_or_else(|| invalid("Publisher profile is missing"))?;
        let external: Option<KeyPins> = previous_pins_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(invalid)?;
        self.with_connection(|connection| accept(connection, &domain, &profile, external.as_ref()))
    }

    /// Publisher context comes only from an authenticated Host selection, never a Guest.
    pub fn resolve_signed_origin(
        &self,
        origin: String,
        publisher_identity: Option<String>,
        migration_approval: Option<String>,
    ) -> Result<String, Error> {
        self.with_connection(|connection| {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if publisher_identity.is_none() {
                let delegated: bool = transaction.query_row(
                    "SELECT EXISTS(SELECT 1 FROM guest_storage_aliases WHERE alias = ?1)",
                    [&origin],
                    |row| row.get(0),
                )?;
                if !delegated {
                    return Ok(origin.clone());
                }
            }
            let signed = SignedOrigin::parse(&origin)?
                .ok_or_else(|| invalid("Publisher context requires a canonical signed origin"))?;
            let canonical = resolve_context(
                &transaction,
                &origin,
                &signed,
                publisher_identity.as_deref(),
                migration_approval.as_deref(),
            )?;
            transaction.commit()?;
            Ok(canonical)
        })
    }

    pub fn storage_migration_token(
        &self,
        origin: String,
        publisher_identity: String,
    ) -> Result<Option<String>, Error> {
        let signed = SignedOrigin::parse(&origin)?
            .ok_or_else(|| invalid("Signed storage origin is required"))?;
        self.with_connection(|connection| {
            let transaction = connection.transaction()?;
            let saved = validated_publisher(&transaction, &signed, &publisher_identity)?;
            let established: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM guest_storage_programs WHERE publisher_identity = ?1 AND program = ?2)", params![saved.identity, signed.program], |row| row.get(0))?;
            if established { return Ok(None); }
            let canonical = format!("publisher:{}:{}", saved.identity, signed.program);
            let aliases = program_aliases(&transaction, &signed, &canonical)?;
            migration_token(&transaction, &saved.identity, signed.program, &aliases)
        })
    }

    /// Explicitly start a separate Publisher space without adopting signer-scoped data.
    pub fn keep_storage_separate(
        &self,
        origin: String,
        publisher_identity: String,
    ) -> Result<String, Error> {
        let signed = SignedOrigin::parse(&origin)?
            .ok_or_else(|| invalid("Signed storage origin is required"))?;
        self.with_connection(|connection| {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let saved = validated_publisher(&transaction, &signed, &publisher_identity)?;
            let canonical = format!("publisher:{}:{}", saved.identity, signed.program);
            provision_program(&transaction, &saved.identity, signed.program, &canonical)?;
            transaction.commit()?;
            Ok(canonical)
        })
    }
}

fn validated_publisher(
    connection: &Connection,
    signed: &SignedOrigin<'_>,
    identity: &str,
) -> Result<SavedPublisher, Error> {
    let saved = load(connection, signed.domain)?
        .ok_or_else(|| invalid("No accepted Publisher identity"))?;
    if saved.identity != identity || !accepts_signer(connection, &saved.profile, signed)? {
        return Err(invalid(
            "The package does not belong to the selected Publisher identity",
        ));
    }
    Ok(saved)
}

fn resolve_context(
    connection: &Connection,
    raw: &str,
    signed: &SignedOrigin<'_>,
    identity: Option<&str>,
    approval: Option<&str>,
) -> Result<String, Error> {
    let delegated: Option<(String, String)> = connection
        .query_row(
            "SELECT origin, publisher_identity FROM guest_storage_aliases WHERE alias = ?1",
            [raw],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some(identity) = identity.or(delegated.as_ref().map(|(_, identity)| identity.as_str()))
    else {
        return Ok(raw.into());
    };
    let saved = validated_publisher(connection, signed, identity)?;
    let canonical = format!("publisher:{}:{}", saved.identity, signed.program);
    if delegated
        .as_ref()
        .is_some_and(|(origin, _)| origin != &canonical)
    {
        return Err(Error::MigrationConflict(
            "The origin belongs to another accepted identity".into(),
        ));
    }
    let established: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM guest_storage_programs WHERE publisher_identity = ?1 AND program = ?2)", params![identity, signed.program], |row| row.get(0))?;
    if !established || approval.is_some() {
        bind_program(connection, identity, signed, &canonical, approval)?;
    }
    Ok(canonical)
}

fn provision_program(
    connection: &Connection,
    identity: &str,
    program: &str,
    canonical: &str,
) -> Result<(), Error> {
    connection.execute("INSERT OR IGNORE INTO guest_storage_programs (publisher_identity, program, origin) VALUES (?1, ?2, ?3)", params![identity, program, canonical])?;
    Ok(())
}

fn accept(
    connection: &mut Connection,
    domain: &str,
    profile: &PublisherProfile,
    external: Option<&KeyPins>,
) -> Result<PublisherProfileAcceptance, Error> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let previous = load(&transaction, domain)?;
    let now = Utc::now();
    let (keys, outcome) = profile
        .accept(
            domain,
            now,
            previous.as_ref().map(|saved| &saved.pins).or(external),
        )
        .map_err(invalid)?;
    let KeyPinOutcome::Accepted(pins) = outcome else {
        return Err(Error::IdentityChanged);
    };
    if previous.is_some()
        && let Some(external) = external
    {
        let (_, outcome) = profile
            .accept(domain, now, Some(external))
            .map_err(invalid)?;
        if !matches!(outcome, KeyPinOutcome::Accepted(_)) {
            return Err(Error::IdentityChanged);
        }
    }
    let identity = previous.map(|saved| saved.identity).unwrap_or_else(|| {
        let mut bytes = [0; 16];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    });
    save(
        &transaction,
        domain,
        &identity,
        &pins,
        profile,
        &keys.keys,
        &now.to_rfc3339(),
    )?;
    transaction.commit()?;
    Ok(PublisherProfileAcceptance {
        profile: profile_info(domain, keys, identity),
        pins_json: serde_json::to_string(&pins).map_err(invalid)?,
    })
}

fn profile_info(domain: &str, keys: PublisherKeys, identity: String) -> PublisherProfileInfo {
    PublisherProfileInfo {
        domain: domain.into(),
        storage_identity: Some(identity),
        profile_sequence: keys.sequence,
        profile_expires_at: keys.profile_expires_at,
        package_signing_keys: keys
            .keys
            .iter()
            .filter(|key| key.verification_key().is_ok())
            .map(|key| PublisherPackageKeyInfo {
                id: key.id.clone(),
                public_key_base64: key.public_key.clone(),
                status: match key.status {
                    actr_protocol::KeyStatus::Active => "active",
                    actr_protocol::KeyStatus::Retired => "retired",
                }
                .into(),
            })
            .collect(),
    }
}

fn accepts_signer(
    connection: &Connection,
    profile: &PublisherProfile,
    signed: &SignedOrigin<'_>,
) -> Result<bool, Error> {
    let keys: Vec<PackageSigningKey> = serde_json::from_value(
        profile
            .other
            .get("keys")
            .cloned()
            .ok_or_else(|| invalid("Stored Publisher has no package keys"))?,
    )
    .map_err(invalid)?;
    if let Some(key) = keys.iter().find(|key| key.public_key == signed.key) {
        key.verification_key().map_err(invalid)?;
        return Ok(true);
    }
    let known: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM guest_publisher_keys WHERE domain = ?1 AND public_key = ?2)",
        params![signed.domain, signed.key],
        |row| row.get(0),
    )?;
    if known {
        return Err(invalid(
            "The package signing key was removed from its Publisher",
        ));
    }
    Ok(false)
}

fn save(
    connection: &Connection,
    domain: &str,
    identity: &str,
    pins: &KeyPins,
    profile: &PublisherProfile,
    keys: &[PackageSigningKey],
    accepted_at: &str,
) -> Result<(), Error> {
    connection.execute(
        "INSERT INTO guest_publishers (domain, identity, pins, profile, accepted_at) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(domain) DO UPDATE SET pins = excluded.pins, profile = excluded.profile, accepted_at = excluded.accepted_at",
        params![domain, identity, serde_json::to_string(pins).map_err(invalid)?, serde_json::to_string(profile).map_err(invalid)?, accepted_at],
    )?;
    for key in keys {
        connection.execute(
            "INSERT OR IGNORE INTO guest_publisher_keys (domain, public_key) VALUES (?1, ?2)",
            params![domain, key.public_key],
        )?;
    }
    Ok(())
}

struct SignedOrigin<'a> {
    key: &'a str,
    program: &'a str,
    domain: &'a str,
}

impl<'a> SignedOrigin<'a> {
    fn parse(origin: &'a str) -> Result<Option<Self>, Error> {
        let Some(rest) = origin.strip_prefix("signed:") else {
            return Ok(None);
        };
        let (key, program) = rest
            .split_once(':')
            .ok_or_else(|| invalid("Invalid signed storage origin"))?;
        let (domain, name) = program
            .split_once('/')
            .ok_or_else(|| invalid("Invalid program identity"))?;
        if BASE64
            .decode(key)
            .ok()
            .is_none_or(|bytes| bytes.len() != 32)
            || name.is_empty()
            || name.contains(['/', '@', ':'])
        {
            return Err(invalid("Invalid signing key or program identity"));
        }
        // Non-network Host identities retain their original signer-scoped storage.
        if canonical_domain(domain).is_err() {
            return Ok(None);
        }
        Ok(Some(Self {
            key,
            program,
            domain,
        }))
    }
}

fn bind_program(
    connection: &Connection,
    identity: &str,
    signed: &SignedOrigin<'_>,
    canonical: &str,
    migration_approval: Option<&str>,
) -> Result<(), Error> {
    let aliases = program_aliases(connection, signed, canonical)?;
    if let Some(token) = migration_token(connection, identity, signed.program, &aliases)? {
        if migration_approval != Some(token.as_str()) {
            return Err(Error::MigrationRequired);
        }
        let sources = nonempty_sources(connection, &aliases)?;
        migrate(connection, canonical, &sources)?;
        // Only an explicit transfer delegates an independently trusted signer space.
        for alias in sources {
            connection.execute("INSERT OR IGNORE INTO guest_storage_aliases (alias, origin, publisher_identity, program) VALUES (?1, ?2, ?3, ?4)", params![alias, canonical, identity, signed.program])?;
        }
    }
    provision_program(connection, identity, signed.program, canonical)
}

fn migration_token(
    connection: &Connection,
    identity: &str,
    program: &str,
    aliases: &[String],
) -> Result<Option<String>, Error> {
    let sources = nonempty_sources(connection, aliases)?;
    if sources.is_empty() {
        return Ok(None);
    }
    let payload = serde_json::to_vec(&(identity, program, sources)).map_err(invalid)?;
    Ok(Some(URL_SAFE_NO_PAD.encode(Sha256::digest(payload))))
}

fn nonempty_sources(connection: &Connection, aliases: &[String]) -> Result<Vec<String>, Error> {
    let mut sources = Vec::new();
    for alias in aliases {
        let nonempty: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM guest_local_storage WHERE origin = ?1)",
            [alias],
            |row| row.get(0),
        )?;
        if nonempty {
            sources.push(alias.clone());
        }
    }
    Ok(sources)
}

fn program_aliases(
    connection: &Connection,
    signed: &SignedOrigin<'_>,
    canonical: &str,
) -> Result<Vec<String>, Error> {
    let mut query = connection.prepare(
        "SELECT public_key FROM guest_publisher_keys WHERE domain = ?1 ORDER BY public_key",
    )?;
    let aliases = query
        .query_map([signed.domain], |row| row.get::<_, String>(0))?
        .map(|key| key.map(|key| format!("signed:{key}:{}", signed.program)))
        .collect::<Result<Vec<_>, _>>()?;
    for alias in &aliases {
        let existing: Option<String> = connection
            .query_row(
                "SELECT origin FROM guest_storage_aliases WHERE alias = ?1",
                [alias],
                |row| row.get(0),
            )
            .optional()?;
        if existing
            .as_deref()
            .is_some_and(|origin| origin != canonical)
        {
            return Err(Error::MigrationConflict(
                "The origin belongs to another accepted identity".into(),
            ));
        }
    }
    Ok(aliases)
}

fn migrate(connection: &Connection, canonical: &str, aliases: &[String]) -> Result<(), Error> {
    let values = collect_values(connection, canonical, aliases)?;
    for (key, value) in values {
        connection.execute("INSERT INTO guest_local_storage (origin, key, value, size) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(origin, key) DO UPDATE SET size = excluded.size", params![canonical, key, value, key.len() + value.len()])?;
    }
    for origin in aliases {
        connection.execute(
            "DELETE FROM guest_local_storage WHERE origin = ?1",
            [origin],
        )?;
    }
    Ok(())
}

fn collect_values(
    connection: &Connection,
    canonical: &str,
    aliases: &[String],
) -> Result<BTreeMap<String, String>, Error> {
    let mut values = BTreeMap::new();
    let mut bytes = 0;
    for origin in std::iter::once(canonical).chain(aliases.iter().map(String::as_str)) {
        let mut query =
            connection.prepare("SELECT key, value FROM guest_local_storage WHERE origin = ?1")?;
        let rows = query.query_map([origin], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (key, value) = row?;
            if key.len() > MAX_KEY_BYTES {
                return Err(Error::InvalidKey);
            }
            if value.len() > MAX_VALUE_BYTES {
                return Err(Error::InvalidValue);
            }
            if let Some(previous) = values.get(&key) {
                if previous != &value {
                    return Err(Error::MigrationConflict(
                        "Existing keys contain different values".into(),
                    ));
                }
            } else {
                bytes += key.len() + value.len();
                values.insert(key, value);
            }
            if bytes > MAX_ORIGIN_BYTES || values.len() > MAX_ORIGIN_ENTRIES {
                return Err(Error::QuotaExceeded);
            }
        }
    }
    Ok(values)
}
