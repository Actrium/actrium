//! Service lookup rules for opening an instance.
//!
//! A Realm has exactly one Service: the only Actor in the Realm whose Actor
//! type equals the package type. Finding zero or more than one is an error;
//! the Actr Host never picks one. An Actor link names the Actor directly, and
//! that Actor's type must equal the package type.

use actr_protocol::{ActrError, ActrId, ActrType, Realm};
use thiserror::Error;

/// Candidates requested when looking up a Service. Discovery may also return
/// Actors of other Realms that an ACL rule exposes, so all are requested and
/// the ones outside the Realm are dropped before counting.
pub const SERVICE_CANDIDATE_COUNT: u32 = u32::MAX;

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

/// Picks the Service in `realm` from discovery candidates for `package_type`.
pub fn select_service(
    package_type: &ActrType,
    realm: &Realm,
    candidates: Vec<ActrId>,
) -> Result<ActrId, ServiceLookupError> {
    let mut candidates = candidates
        .into_iter()
        .filter(|candidate| candidate.realm == *realm);
    match (candidates.next(), candidates.next()) {
        (Some(service), None) => Ok(service),
        (None, _) => Err(ServiceLookupError::NotFound(package_type.to_string())),
        (Some(_), Some(_)) => Err(ServiceLookupError::NotUnique(package_type.to_string())),
    }
}

/// Checks that the Actor named by an Actor link runs the package type.
pub fn check_service_actor(
    package_type: &ActrType,
    actor: &ActrId,
) -> Result<(), ServiceLookupError> {
    if actor.r#type == *package_type {
        Ok(())
    } else {
        Err(ServiceLookupError::ActorTypeMismatch {
            actor: actor.to_string(),
            expected: package_type.to_string(),
            actual: actor.r#type.to_string(),
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

    const REALM: Realm = Realm { realm_id: 7 };

    fn actor(serial_number: u64, actr_type: ActrType) -> ActrId {
        ActrId {
            realm: REALM,
            serial_number,
            r#type: actr_type,
        }
    }

    #[test]
    fn selects_the_only_candidate() {
        let service = actor(1, actr_type("1.0.0"));
        assert_eq!(
            select_service(&actr_type("1.0.0"), &REALM, vec![service.clone()]),
            Ok(service)
        );
    }

    #[test]
    fn zero_candidates_is_service_not_found() {
        let error = select_service(&actr_type("1.0.0"), &REALM, vec![]).unwrap_err();
        assert_eq!(
            error,
            ServiceLookupError::NotFound(actr_type("1.0.0").to_string())
        );
        assert_eq!(error.code(), "SERVICE_NOT_FOUND");
    }

    #[test]
    fn more_than_one_candidate_is_an_error() {
        let candidates = vec![actor(1, actr_type("1.0.0")), actor(2, actr_type("1.0.0"))];
        let error = select_service(&actr_type("1.0.0"), &REALM, candidates).unwrap_err();
        assert_eq!(
            error,
            ServiceLookupError::NotUnique(actr_type("1.0.0").to_string())
        );
        assert_eq!(error.code(), "SERVICE_NOT_FOUND");
    }

    #[test]
    fn ignores_candidates_in_other_realms() {
        let other_realm = |serial_number| ActrId {
            realm: Realm { realm_id: 8 },
            ..actor(serial_number, actr_type("1.0.0"))
        };

        let service = actor(1, actr_type("1.0.0"));
        assert_eq!(
            select_service(
                &actr_type("1.0.0"),
                &REALM,
                vec![other_realm(2), service.clone(), other_realm(3)]
            ),
            Ok(service)
        );

        let error = select_service(&actr_type("1.0.0"), &REALM, vec![other_realm(2)]).unwrap_err();
        assert_eq!(error.code(), "SERVICE_NOT_FOUND");
    }

    #[test]
    fn actor_link_must_match_package_type() {
        assert_eq!(
            check_service_actor(&actr_type("1.0.0"), &actor(1, actr_type("1.0.0"))),
            Ok(())
        );

        let error =
            check_service_actor(&actr_type("1.0.0"), &actor(1, actr_type("2.0.0"))).unwrap_err();
        assert_eq!(error.code(), "SERVICE_ACTOR_TYPE_MISMATCH");
    }

    #[test]
    fn converts_to_actr_error_with_code() {
        let error: ActrError = ServiceLookupError::NotUnique(actr_type("1.0.0").to_string()).into();
        assert!(
            matches!(&error, ActrError::NotFound(message) if message.starts_with("SERVICE_NOT_FOUND: "))
        );

        let error: ActrError =
            check_service_actor(&actr_type("1.0.0"), &actor(1, actr_type("2.0.0")))
                .unwrap_err()
                .into();
        assert!(
            matches!(&error, ActrError::InvalidArgument(message) if message.starts_with("SERVICE_ACTOR_TYPE_MISMATCH: "))
        );
    }
}
