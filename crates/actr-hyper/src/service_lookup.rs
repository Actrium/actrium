//! Service lookup rules for opening an instance.
//!
//! A Realm has exactly one Service: the only Actor in the Realm whose Actor
//! type equals the package type. Finding zero or more than one is an error;
//! the Actr Host never picks one. An Actor link names the Actor directly, and
//! that Actor's type must equal the package type.

use actr_protocol::{ActrError, ActrId, ActrType};
use thiserror::Error;

/// Signaling scopes this lookup to the requested Realm; two candidates suffice
/// to distinguish missing, unique, and non-unique Services.
pub const SERVICE_CANDIDATE_COUNT: u32 = 2;

/// Service lookup failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ServiceLookupError {
    #[error("no Actor of type {0} in this Realm")]
    NotFound(String),

    #[error("more than one Actor of type {0} in this Realm")]
    NotUnique(String),

    #[error("Actor {actor} has type {actual}, but the package type is {expected}")]
    ActorTypeMismatch {
        actor: String,
        expected: String,
        actual: String,
    },
}

impl ServiceLookupError {
    /// Stable error code for logs, tests, and developer tools.
    ///
    /// `SERVICE_NOT_FOUND` covers both zero and more than one candidate.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) | Self::NotUnique(_) => "SERVICE_NOT_FOUND",
            Self::ActorTypeMismatch { .. } => "SERVICE_ACTOR_TYPE_MISMATCH",
        }
    }
}

impl From<ServiceLookupError> for ActrError {
    fn from(error: ServiceLookupError) -> Self {
        let message = format!("{}: {error}", error.code());
        match error {
            ServiceLookupError::NotFound(_) | ServiceLookupError::NotUnique(_) => {
                ActrError::NotFound(message)
            }
            ServiceLookupError::ActorTypeMismatch { .. } => ActrError::InvalidArgument(message),
        }
    }
}

/// Picks the Service in `realm` (a Realm name) from discovery candidates for
/// `package_type`.
pub fn select_service(
    package_type: &ActrType,
    realm: &str,
    candidates: Vec<ActrId>,
) -> Result<ActrId, ServiceLookupError> {
    let mut candidates = candidates
        .into_iter()
        .filter(|candidate| candidate.realm == realm);
    match (candidates.next(), candidates.next()) {
        (Some(service), None) => Ok(service),
        (None, _) => Err(ServiceLookupError::NotFound(package_type.to_string())),
        (Some(_), Some(_)) => Err(ServiceLookupError::NotUnique(package_type.to_string())),
    }
}

/// Checks that the Actor named by an Actor link runs the package type.
///
/// `actual_type` is the Actor's verified type (from its credential); an
/// `ActrId` does not carry a type.
pub fn check_service_actor(
    package_type: &ActrType,
    actor: &ActrId,
    actual_type: &ActrType,
) -> Result<(), ServiceLookupError> {
    if actual_type == package_type {
        Ok(())
    } else {
        Err(ServiceLookupError::ActorTypeMismatch {
            actor: actor.to_string(),
            expected: package_type.to_string(),
            actual: actual_type.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actr_type(version: &str) -> ActrType {
        ActrType {
            publisher: "acme.example".to_string(),
            name: "notes".to_string(),
            version: version.to_string(),
        }
    }

    const REALM: &str = "test@example.com";

    fn actor(serial_number: u64) -> ActrId {
        ActrId {
            realm: REALM.to_string(),
            serial_number,
        }
    }

    #[test]
    fn selects_the_only_candidate() {
        let service = actor(1);
        assert_eq!(
            select_service(&actr_type("1.0.0"), REALM, vec![service.clone()]),
            Ok(service)
        );
    }

    #[test]
    fn zero_candidates_is_service_not_found() {
        let error = select_service(&actr_type("1.0.0"), REALM, vec![]).unwrap_err();
        assert_eq!(
            error,
            ServiceLookupError::NotFound(actr_type("1.0.0").to_string())
        );
        assert_eq!(error.code(), "SERVICE_NOT_FOUND");
    }

    #[test]
    fn more_than_one_candidate_is_an_error() {
        let candidates = vec![actor(1), actor(2)];
        let error = select_service(&actr_type("1.0.0"), REALM, candidates).unwrap_err();
        assert_eq!(
            error,
            ServiceLookupError::NotUnique(actr_type("1.0.0").to_string())
        );
        assert_eq!(error.code(), "SERVICE_NOT_FOUND");
    }

    #[test]
    fn ignores_candidates_in_other_realms() {
        let other_realm = |serial_number| ActrId {
            realm: "other@example.com".to_string(),
            serial_number,
        };

        let service = actor(1);
        assert_eq!(
            select_service(
                &actr_type("1.0.0"),
                REALM,
                vec![other_realm(2), service.clone(), other_realm(3)]
            ),
            Ok(service)
        );

        let error = select_service(&actr_type("1.0.0"), REALM, vec![other_realm(2)]).unwrap_err();
        assert_eq!(error.code(), "SERVICE_NOT_FOUND");
    }

    #[test]
    fn actor_link_must_match_package_type() {
        assert_eq!(
            check_service_actor(&actr_type("1.0.0"), &actor(1), &actr_type("1.0.0")),
            Ok(())
        );

        let error =
            check_service_actor(&actr_type("1.0.0"), &actor(1), &actr_type("2.0.0")).unwrap_err();
        assert_eq!(error.code(), "SERVICE_ACTOR_TYPE_MISMATCH");
    }

    #[test]
    fn converts_to_actr_error_with_code() {
        let error: ActrError = ServiceLookupError::NotUnique(actr_type("1.0.0").to_string()).into();
        assert!(
            matches!(&error, ActrError::NotFound(message) if message.starts_with("SERVICE_NOT_FOUND: "))
        );

        let error: ActrError =
            check_service_actor(&actr_type("1.0.0"), &actor(1), &actr_type("2.0.0"))
                .unwrap_err()
                .into();
        assert!(
            matches!(&error, ActrError::InvalidArgument(message) if message.starts_with("SERVICE_ACTOR_TYPE_MISMATCH: "))
        );
    }
}
