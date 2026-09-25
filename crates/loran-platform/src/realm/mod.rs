//! Realm Management Module
//!
//! Realms hosted by this instance: lifecycle, secrets and connection access control.

// Submodules
pub mod acl;
pub mod changes;
pub mod domain;
pub mod error;
pub mod issuer_key;
pub mod management;
pub mod model;
pub mod profile;
pub mod secret;
pub mod turn_secret;
pub mod validation;

// Public API exports
pub use acl::ActorAcl;
pub use domain::RealmDomain;
pub use error::RealmError;
pub use issuer_key::{IssuerKeyState, RealmIssuerKey};
pub use model::{Realm, RealmStatus};
pub use profile::RealmProfileRecord;
pub use secret::{
    DEFAULT_REALM_SECRET_PREVIOUS_GRACE_SECS, REALM_SECRET_HEADER, RealmSecretCheck,
    RealmSecretRotation, RealmSecretState, get_realm_secret_state, hash_realm_secret,
    rotate_realm_secret, verify_realm_secret,
};
pub use turn_secret::realm_turn_secret;
pub use validation::RealmValidationError;
