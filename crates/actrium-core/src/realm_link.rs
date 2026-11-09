//! Installation-aware guards for opening and sharing Realm links.
//!
//! This module does not implement Realm discovery or the Front-Service flow.
//! It ensures that the future open path cannot accidentally perform local
//! record access, state mutation, or network discovery before ownership is
//! validated.

use actr_protocol::uri::{ActrUri, ActrUriError};

/// Parse a Realm link, validate local installation ownership, and select its
/// resolution path.
///
/// A matching local Realm is passed only to `open_local`, whose implementation
/// must resolve the installation's local record without network discovery. A
/// DNS Realm is passed only to `discover_remote`. A foreign local Realm fails
/// with [`ActrUriError::LocalRealmForeign`] before either callback is entered.
pub fn open_realm_link<T, E>(
    input: &str,
    installation_id: &str,
    open_local: impl FnOnce(&ActrUri) -> Result<T, E>,
    discover_remote: impl FnOnce(&ActrUri) -> Result<T, E>,
) -> Result<T, E>
where
    E: From<ActrUriError>,
{
    let uri = ActrUri::parse_for_installation(input, installation_id).map_err(E::from)?;
    if uri.is_local() {
        open_local(&uri)
    } else {
        discover_remote(&uri)
    }
}

/// Whether a Realm link may be shared with another installation.
///
/// This check is deliberately independent of installation ownership so every
/// local Realm, including a foreign one, has sharing disabled before opening
/// or discovery is attempted. Malformed links are not shareable either.
pub fn can_share_realm_link(input: &str) -> bool {
    input.parse::<ActrUri>().is_ok_and(|uri| uri.is_shareable())
}
