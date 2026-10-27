//! ACL (Access Control List) permission checking
//!
//! Starting from the caller of an inbound message (its ActorId and Actor type)
//! and the target actor_id, determines whether the call is permitted based on
//! configured ACL rules. The ActorId carries no type, so the caller's type is
//! passed separately; it comes from verified credential claims.
//! This module is pure functions with no IO dependencies, suitable for
//! both native and wasm32 targets.

use actr_protocol::{Acl, AclRule, ActrId, ActrType};

/// Check whether the caller has permission to access the target Actor
///
/// # Returns
/// - `Ok(true)`: allowed
/// - `Ok(false)`: denied
/// - `Err(String)`: check error (should be treated as denied)
///
/// `caller_type` is the caller's verified Actor type; `None` when it is not
/// known, in which case no rule matches.
///
/// # Evaluation logic
/// 1. No caller_id (local call) -- always allow
/// 2. No ACL configured -- allow by default (backward compatibility)
/// 3. ACL configured but rules list empty -- deny all (secure default)
/// 4. Deny-first: any matching DENY rule immediately denies
/// 5. At least one matching ALLOW rule -- allow
/// 6. No rule matches -- deny
pub fn check_acl_permission(
    caller_id: Option<&ActrId>,
    caller_type: Option<&ActrType>,
    target_id: &ActrId,
    acl: Option<&Acl>,
) -> Result<bool, String> {
    // 1. Local calls are always allowed
    if caller_id.is_none() {
        tracing::trace!("ACL: local call, allowing");
        return Ok(true);
    }

    let caller = caller_id.unwrap();

    // 2. No ACL configured -- allow by default
    let acl = match acl {
        Some(a) => a,
        None => {
            tracing::trace!(
                "ACL: no ACL configured, allowing {} -> {}",
                caller,
                target_id,
            );
            return Ok(true);
        }
    };

    // 3. Empty rules list -- deny all
    if acl.rules.is_empty() {
        tracing::warn!(
            "ACL: empty rule set, denying {} -> {} (default deny)",
            caller,
            target_id,
        );
        return Ok(false);
    }

    // 4 & 5. Deny-first evaluation
    let mut any_allow = false;
    for rule in &acl.rules {
        if !matches_rule(caller, caller_type, rule) {
            continue;
        }
        let is_allow = rule.permission == actr_protocol::acl_rule::Permission::Allow as i32;
        if !is_allow {
            tracing::debug!("ACL: DENY rule matched for {} -> {}", caller, target_id,);
            return Ok(false);
        }
        any_allow = true;
    }

    if any_allow {
        tracing::debug!("ACL: ALLOW rule matched for {} -> {}", caller, target_id,);
        return Ok(true);
    }

    // 6. No rule matches -- deny
    tracing::warn!(
        "ACL: no matching rule, denying {} -> {} (default deny)",
        caller,
        target_id,
    );
    Ok(false)
}

/// Check whether a single ACL rule matches the given caller
fn matches_rule(caller: &ActrId, caller_type: Option<&ActrType>, rule: &AclRule) -> bool {
    use actr_protocol::acl_rule::SourceRealm;

    // Exact type match (publisher + name + version)
    if caller_type != Some(&rule.from_type) {
        return false;
    }

    // Realm match
    match &rule.source_realm {
        None | Some(SourceRealm::AnyRealm(_)) => true,
        Some(SourceRealm::Realm(realm)) => caller.realm == *realm,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actr_protocol::{acl_rule::Permission, acl_rule::SourceRealm};

    fn make_id(realm: &str) -> ActrId {
        ActrId {
            serial_number: 0xaabb,
            realm: realm.into(),
        }
    }

    fn make_type(publisher: &str, name: &str, version: &str) -> ActrType {
        ActrType {
            publisher: publisher.into(),
            name: name.into(),
            version: version.into(),
        }
    }

    fn make_rule(publisher: &str, name: &str, version: &str, perm: Permission) -> AclRule {
        AclRule {
            permission: perm as i32,
            from_type: ActrType {
                publisher: publisher.into(),
                name: name.into(),
                version: version.into(),
            },
            source_realm: None,
        }
    }

    const REALM: &str = "k7m2q9xh@example.com";

    #[test]
    fn local_call_always_allowed() {
        let target = make_id(REALM);
        assert!(check_acl_permission(None, None, &target, None).unwrap());
    }

    #[test]
    fn no_acl_allows_by_default() {
        let caller = make_id(REALM);
        let caller_type = make_type("acme", "client", "0.1.0");
        let target = make_id(REALM);
        assert!(check_acl_permission(Some(&caller), Some(&caller_type), &target, None).unwrap());
    }

    #[test]
    fn empty_rules_denies() {
        let caller = make_id(REALM);
        let caller_type = make_type("acme", "client", "0.1.0");
        let target = make_id(REALM);
        let acl = Acl { rules: vec![] };
        assert!(
            !check_acl_permission(Some(&caller), Some(&caller_type), &target, Some(&acl)).unwrap()
        );
    }

    #[test]
    fn deny_overrides_allow() {
        let caller = make_id(REALM);
        let caller_type = make_type("acme", "client", "0.1.0");
        let target = make_id(REALM);
        let acl = Acl {
            rules: vec![
                make_rule("acme", "client", "0.1.0", Permission::Allow),
                make_rule("acme", "client", "0.1.0", Permission::Deny),
            ],
        };
        assert!(
            !check_acl_permission(Some(&caller), Some(&caller_type), &target, Some(&acl)).unwrap()
        );
    }

    #[test]
    fn allow_when_matched() {
        let caller = make_id(REALM);
        let caller_type = make_type("acme", "client", "0.1.0");
        let target = make_id(REALM);
        let acl = Acl {
            rules: vec![make_rule("acme", "client", "0.1.0", Permission::Allow)],
        };
        assert!(
            check_acl_permission(Some(&caller), Some(&caller_type), &target, Some(&acl)).unwrap()
        );
    }

    #[test]
    fn unknown_caller_type_matches_no_rule() {
        let caller = make_id(REALM);
        let target = make_id(REALM);
        let acl = Acl {
            rules: vec![make_rule("acme", "client", "0.1.0", Permission::Allow)],
        };
        assert!(!check_acl_permission(Some(&caller), None, &target, Some(&acl)).unwrap());
    }

    #[test]
    fn no_match_denies() {
        let caller = make_id(REALM);
        let caller_type = make_type("acme", "client", "0.1.0");
        let target = make_id(REALM);
        let acl = Acl {
            rules: vec![make_rule("other", "other", "0.1.0", Permission::Allow)],
        };
        assert!(
            !check_acl_permission(Some(&caller), Some(&caller_type), &target, Some(&acl)).unwrap()
        );
    }

    #[test]
    fn realm_rule_matches_by_realm_name() {
        let caller_type = make_type("acme", "client", "0.1.0");
        let target = make_id(REALM);
        let acl = Acl {
            rules: vec![AclRule {
                source_realm: Some(SourceRealm::Realm("other@example.com".into())),
                ..make_rule("acme", "client", "0.1.0", Permission::Allow)
            }],
        };
        let other = make_id("other@example.com");
        assert!(
            check_acl_permission(Some(&other), Some(&caller_type), &target, Some(&acl)).unwrap()
        );
        let same = make_id(REALM);
        assert!(
            !check_acl_permission(Some(&same), Some(&caller_type), &target, Some(&acl)).unwrap()
        );
    }

    #[test]
    fn any_realm_rule_matches_foreign_realm() {
        let caller = make_id("other@example.com");
        let caller_type = make_type("acme", "client", "0.1.0");
        let target = make_id(REALM);
        let acl = Acl {
            rules: vec![AclRule {
                source_realm: Some(SourceRealm::AnyRealm(true)),
                ..make_rule("acme", "client", "0.1.0", Permission::Allow)
            }],
        };
        assert!(
            check_acl_permission(Some(&caller), Some(&caller_type), &target, Some(&acl)).unwrap()
        );
    }
}
