//! Preparatory guards for opening and sharing Realm links.
//!
//! These helpers validate installation ownership and select a supplied local
//! or remote resolver. Production open/share integration remains tracked by
//! <https://github.com/Actrium/actrium/issues/400> as part of the Front-Service
//! flow in <https://github.com/Actrium/actrium/issues/324>.

use actr_protocol::uri::{ActrUri, ActrUriError};

/// Parse a Realm link, validate local installation ownership, and select its
/// resolution path.
///
/// A matching local Realm is passed only to `open_local`, whose implementation
/// must resolve the installation's local record without network discovery. A
/// DNS Realm is passed only to `discover_remote`. A foreign local Realm fails
/// with [`ActrUriError::LocalRealmForeign`] before either callback is entered.
/// An authority-only URI has no RealmId and fails with
/// [`ActrUriError::InvalidLabel`] before either callback is entered.
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
    if uri.realm_id().is_none() {
        return Err(E::from(ActrUriError::InvalidLabel(
            "a Realm link must include a RealmId".to_string(),
        )));
    }
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
