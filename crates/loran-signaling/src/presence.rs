//! Presence subscription management module
//!
//! Responsible for managing the subscription and notification mechanism
//! for Actor online events
//!
//! # Features
//! - Subscribe to online events for a specific ActrType
//! - Unsubscribe
//! - Notify all subscribers when a new Actor registers
//!
//! # Usage example
//! ```ignore
//! use signaling::presence::PresenceManager;
//! use actr_protocol::{ActrId, ActrType};
//!
//! let mut manager = PresenceManager::new();
//!
//! let actor_a_id = ActrId { /* ... */ };
//! let actor_a_type = ActrType { /* ... */ };
//! let user_service_type = ActrType {
//!     publisher: "acme".to_string(),
//!     name: "user-service".to_string(),
//!     version: "1.0.0".to_string(),
//! };
//!
//! // Actor A subscribes to online events for the user-service type
//! manager.subscribe(actor_a_id, actor_a_type, user_service_type.clone(), member);
//!
//! // When a new user-service instance registers
//! let subscribers = manager.get_subscribers(&actor_a_id.realm, &user_service_type);
//! // Push ActrUpEvent to subscribers
//! ```

use actr_protocol::{ActrId, ActrType};
use platform::RealmError;
use platform::realm::acl::ActorAcl;
use std::collections::{HashMap, HashSet};

/// Presence subscription manager
///
/// Partitioned by Realm: an Actor subscribes to a type within its own Realm
/// and is only told about Actors of that Realm.
/// One presence subscription: the subscriber and its verified identity.
#[derive(Debug, Clone)]
struct Subscription {
    actor_id: ActrId,
    actor_type: ActrType,
    /// The `member` claim of the subscriber's credential. The instance ACL
    /// key for guarded Services (FR-SEC-013): presence of an Actor reaches
    /// only the member that may attach to it.
    member: String,
}

#[derive(Debug, Default)]
pub struct PresenceManager {
    /// Subscription map: (realm, target_type) -> Vec<Subscription>
    ///
    /// Key: the subscriber's Realm and the subscribed service type (ActrType)
    /// Value: the Actors subscribed to that type, with their verified types
    /// and membership
    subscriptions: HashMap<(String, ActrType), Vec<Subscription>>,
    /// Realms retired while this process runs; nobody subscribes in them again
    retired: HashSet<String>,
}

impl PresenceManager {
    /// Create a new PresenceManager
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribe to Actor online events for a specific type
    ///
    /// # Arguments
    /// - `subscriber`: the subscriber's ActrId
    /// - `subscriber_type`: the subscriber's verified Actor type
    /// - `target_type`: the service type to subscribe to
    /// - `member`: the subscriber's verified `member` claim (instance ACL key)
    ///
    /// # Example
    /// ```ignore
    /// manager.subscribe(client_actor_id, client_actor_type, user_service_type, member);
    /// ```
    pub fn subscribe(
        &mut self,
        subscriber: ActrId,
        subscriber_type: ActrType,
        target_type: ActrType,
        member: String,
    ) {
        if self.retired.contains(&subscriber.realm) {
            platform::recording::warn!(
                "Actor {} not subscribed: its Realm is retired",
                subscriber.to_string_repr()
            );
            return;
        }
        platform::recording::info!(
            "Actor {} subscribed to {}/{} online events",
            subscriber.to_string_repr(),
            target_type.publisher,
            target_type.name
        );

        let subscribers = self
            .subscriptions
            .entry((subscriber.realm.clone(), target_type))
            .or_default();

        // Avoid duplicate subscriptions
        if !subscribers.iter().any(|s| s.actor_id == subscriber) {
            subscribers.push(Subscription {
                actor_id: subscriber,
                actor_type: subscriber_type,
                member,
            });
            platform::recording::debug!(
                "Subscription successful, current subscriber count: {}",
                subscribers.len()
            );
        } else {
            platform::recording::warn!(
                "Actor {} has already subscribed to this type",
                subscriber.to_string_repr()
            );
        }
    }

    /// Unsubscribe from Actor online events for a specific type
    ///
    /// # Arguments
    /// - `subscriber`: the subscriber's ActrId
    /// - `target_type`: the service type to unsubscribe from
    ///
    /// # Returns
    /// - `true`: successfully unsubscribed
    /// - `false`: subscription does not exist
    pub fn unsubscribe(&mut self, subscriber: &ActrId, target_type: &ActrType) -> bool {
        platform::recording::info!(
            "Actor {} unsubscribed from {}/{} online events",
            subscriber.to_string_repr(),
            target_type.publisher,
            target_type.name
        );

        let key = (subscriber.realm.clone(), target_type.clone());
        if let Some(subscribers) = self.subscriptions.get_mut(&key) {
            let original_len = subscribers.len();
            subscribers.retain(|s| s.actor_id != *subscriber);

            let removed = subscribers.len() < original_len;
            if removed {
                platform::recording::debug!(
                    "Unsubscribe successful, remaining subscriber count: {}",
                    subscribers.len()
                );

                // If no subscribers remain, remove the entire entry
                if subscribers.is_empty() {
                    self.subscriptions.remove(&key);
                    platform::recording::debug!(
                        "No subscribers left for this type, removing subscription entry"
                    );
                }
            } else {
                platform::recording::warn!(
                    "Actor {} has not subscribed to this type",
                    subscriber.to_string_repr()
                );
            }

            removed
        } else {
            platform::recording::warn!(
                "Type {}/{} has no subscriptions",
                target_type.publisher,
                target_type.name
            );
            false
        }
    }

    /// Unsubscribe a specific Actor from all subscriptions
    ///
    /// Used for cleanup when an Actor goes offline or disconnects
    ///
    /// # Arguments
    /// - `subscriber`: the subscriber's ActrId
    ///
    /// # Returns
    /// The number of subscriptions removed
    pub fn unsubscribe_all(&mut self, subscriber: &ActrId) -> usize {
        platform::recording::info!(
            "Clearing all subscriptions for Actor {}",
            subscriber.to_string_repr()
        );

        let mut removed_count = 0;

        // Remove this subscriber from all subscription lists
        self.subscriptions.retain(|_key, subscribers| {
            let original_len = subscribers.len();
            subscribers.retain(|s| s.actor_id != *subscriber);
            removed_count += original_len - subscribers.len();

            // If the list is empty, return false to remove the entry
            !subscribers.is_empty()
        });

        if removed_count > 0 {
            platform::recording::info!("Cleared {} subscriptions", removed_count);
        }

        removed_count
    }

    /// Drop a retired Realm's subscriptions and refuse new ones
    pub fn retire_realm(&mut self, realm: &str) {
        self.retired.insert(realm.to_string());
        self.subscriptions
            .retain(|(subscribed_realm, _), _| subscribed_realm != realm);
    }

    /// Get all Actors of a Realm subscribed to a specific type
    ///
    /// # Arguments
    /// - `realm`: the Realm
    /// - `target_type`: the service type
    ///
    /// # Returns
    /// A list of subscriber ActrIds (references)
    ///
    /// # Usage
    /// When a new target_type instance registers, use this method to get
    /// all subscribers that need to be notified
    pub fn get_subscribers(&self, realm: &str, target_type: &ActrType) -> Vec<&ActrId> {
        self.subscriptions
            .get(&(realm.to_string(), target_type.clone()))
            .map(|subscribers| subscribers.iter().map(|s| &s.actor_id).collect())
            .unwrap_or_default()
    }

    /// Get subscription statistics
    ///
    /// # Returns
    /// (number of subscribed service types, total subscriber count)
    pub fn stats(&self) -> (usize, usize) {
        let type_count = self.subscriptions.len();
        let subscriber_count: usize = self
            .subscriptions
            .values()
            .map(|subscribers| subscribers.len())
            .sum();

        (type_count, subscriber_count)
    }

    /// Check if a specific Actor is subscribed to a type
    pub fn is_subscribed(&self, subscriber: &ActrId, target_type: &ActrType) -> bool {
        self.subscriptions
            .get(&(subscriber.realm.clone(), target_type.clone()))
            .map(|subscribers| subscribers.iter().any(|s| s.actor_id == *subscriber))
            .unwrap_or(false)
    }

    /// Get subscribers with ACL filtering
    ///
    /// Returns only subscribers of the target actor's Realm that are allowed
    /// to discover it based on ACL rules
    ///
    /// # Arguments
    ///
    /// - `target_actor_id`: The actor that came online
    /// - `target_type`: Its verified Actor type
    ///
    /// # Returns
    ///
    /// List of subscriber ActorIds that passed ACL checks
    pub async fn get_subscribers_with_acl(
        &self,
        target_actor_id: &ActrId,
        target_type: &ActrType,
    ) -> Vec<ActrId> {
        let subscribers = self
            .subscriptions
            .get(&(target_actor_id.realm.clone(), target_type.clone()))
            .map(Vec::as_slice)
            .unwrap_or_default();
        let total_count = subscribers.len(); // Save count before moving

        let mut allowed_subscribers = Vec::new();

        for subscription in subscribers {
            let subscriber_id = &subscription.actor_id;
            // Type-level ACL: can subscriber discover target?
            let type_allowed = match Self::check_discovery_acl(
                subscriber_id,
                &subscription.actor_type,
                target_actor_id,
                target_type,
            )
            .await
            {
                Ok(true) => true,
                Ok(false) => {
                    platform::recording::debug!(
                        "ACL denied discovery notification: subscriber={}, target={}",
                        subscriber_id.to_string_repr(),
                        target_actor_id.to_string_repr()
                    );
                    false
                }
                Err(e) => {
                    platform::recording::warn!(
                        "ACL check failed, denying notification: subscriber={}, target={}, error={}",
                        subscriber_id.to_string_repr(),
                        target_actor_id.to_string_repr(),
                        e
                    );
                    false
                }
            };
            if !type_allowed {
                continue;
            }
            // Instance ACL (FR-SEC-013): the actor_id of an Actor this
            // Provider issued reaches only its owner and grantees — knowing
            // it is the last input a stranger needs for a direct relay
            // attach. A store failure denies the notification.
            match platform::realm::service_acl::service_attachee_allowed(
                target_actor_id,
                &subscription.member,
            )
            .await
            {
                Ok(true) => allowed_subscribers.push(subscriber_id.clone()),
                Ok(false) => {
                    platform::recording::debug!(
                        "Service ACL denied presence: member {} cannot attach to {}",
                        subscription.member,
                        target_actor_id.to_string_repr(),
                    );
                }
                Err(e) => {
                    platform::recording::warn!(
                        "Service ACL check failed, denying notification: member {}, target={}, error={}",
                        subscription.member,
                        target_actor_id.to_string_repr(),
                        e
                    );
                }
            }
        }

        platform::recording::info!(
            "ACL filtering completed for presence notification: target_type={:?}, total_subscribers={}, allowed_subscribers={}",
            target_type,
            total_count,
            allowed_subscribers.len()
        );

        allowed_subscribers
    }

    /// Check if discovery is allowed between two actors based on ACL rules
    ///
    /// # Arguments
    ///
    /// - `from_actor` / `from_type`: Subscriber actor ID and its type
    /// - `to_actor` / `to_type`: Target actor ID and its type
    ///
    /// # Returns
    ///
    /// Returns true if discovery is allowed based on ACL rules
    async fn check_discovery_acl(
        from_actor: &ActrId,
        from_type: &ActrType,
        to_actor: &ActrId,
        to_type: &ActrType,
    ) -> Result<bool, RealmError> {
        ActorAcl::can_discover(
            &from_actor.realm,
            &to_actor.realm,
            &from_type.to_string_repr(),
            &to_type.to_string_repr(),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_actor_id(serial: u64) -> ActrId {
        ActrId {
            serial_number: serial,
            realm: "test@example.com".to_string(),
        }
    }

    fn create_test_actor_type(name: &str) -> ActrType {
        ActrType {
            publisher: "test".to_string(),
            name: name.to_string(),
            version: "1.0.0".to_string(),
        }
    }

    #[test]
    fn test_subscribe_and_get_subscribers() {
        let mut manager = PresenceManager::new();
        let actor1 = create_test_actor_id(1);
        let actor2 = create_test_actor_id(2);
        let target_type = create_test_actor_type("user-service");

        manager.subscribe(
            actor1.clone(),
            create_test_actor_type("test-actor"),
            target_type.clone(),
            "member-1".to_string(),
        );
        manager.subscribe(
            actor2.clone(),
            create_test_actor_type("test-actor"),
            target_type.clone(),
            "member-1".to_string(),
        );

        let subscribers = manager.get_subscribers("test@example.com", &target_type);
        assert_eq!(subscribers.len(), 2);
    }

    #[test]
    fn test_unsubscribe() {
        let mut manager = PresenceManager::new();
        let actor1 = create_test_actor_id(1);
        let target_type = create_test_actor_type("user-service");

        manager.subscribe(
            actor1.clone(),
            create_test_actor_type("test-actor"),
            target_type.clone(),
            "member-1".to_string(),
        );
        assert!(manager.unsubscribe(&actor1, &target_type));

        let subscribers = manager.get_subscribers("test@example.com", &target_type);
        assert_eq!(subscribers.len(), 0);
    }

    #[test]
    fn test_unsubscribe_all() {
        let mut manager = PresenceManager::new();
        let actor1 = create_test_actor_id(1);
        let type1 = create_test_actor_type("user-service");
        let type2 = create_test_actor_type("order-service");

        manager.subscribe(
            actor1.clone(),
            create_test_actor_type("test-actor"),
            type1.clone(),
            "member-1".to_string(),
        );
        manager.subscribe(
            actor1.clone(),
            create_test_actor_type("test-actor"),
            type2.clone(),
            "member-1".to_string(),
        );

        let removed = manager.unsubscribe_all(&actor1);
        assert_eq!(removed, 2);

        assert_eq!(manager.get_subscribers("test@example.com", &type1).len(), 0);
        assert_eq!(manager.get_subscribers("test@example.com", &type2).len(), 0);
    }

    #[test]
    fn test_duplicate_subscription() {
        let mut manager = PresenceManager::new();
        let actor1 = create_test_actor_id(1);
        let target_type = create_test_actor_type("user-service");

        manager.subscribe(
            actor1.clone(),
            create_test_actor_type("test-actor"),
            target_type.clone(),
            "member-1".to_string(),
        );
        manager.subscribe(
            actor1.clone(),
            create_test_actor_type("test-actor"),
            target_type.clone(),
            "member-1".to_string(),
        ); // duplicate subscription

        let subscribers = manager.get_subscribers("test@example.com", &target_type);
        assert_eq!(subscribers.len(), 1); // should only have one
    }

    #[test]
    fn test_stats() {
        let mut manager = PresenceManager::new();
        let actor1 = create_test_actor_id(1);
        let actor2 = create_test_actor_id(2);
        let type1 = create_test_actor_type("user-service");
        let type2 = create_test_actor_type("order-service");

        manager.subscribe(
            actor1.clone(),
            create_test_actor_type("test-actor"),
            type1.clone(),
            "member-1".to_string(),
        );
        manager.subscribe(
            actor2.clone(),
            create_test_actor_type("test-actor"),
            type1.clone(),
            "member-1".to_string(),
        );
        manager.subscribe(
            actor1.clone(),
            create_test_actor_type("test-actor"),
            type2.clone(),
            "member-1".to_string(),
        );

        let (type_count, subscriber_count) = manager.stats();
        assert_eq!(type_count, 2); // 2 types
        assert_eq!(subscriber_count, 3); // 3 subscriptions
    }

    #[test]
    fn subscriptions_stay_within_the_subscribers_realm() {
        let mut manager = PresenceManager::new();
        let target_type = create_test_actor_type("user-service");
        // Same serial and type, two Realms.
        let a = create_test_actor_id(1);
        let b = ActrId {
            realm: "other@example.com".to_string(),
            serial_number: 1,
        };
        manager.subscribe(
            a.clone(),
            create_test_actor_type("test-actor"),
            target_type.clone(),
            "member-1".to_string(),
        );

        assert_eq!(manager.get_subscribers(&a.realm, &target_type), vec![&a]);
        assert!(manager.get_subscribers(&b.realm, &target_type).is_empty());
        assert!(manager.is_subscribed(&a, &target_type));
        assert!(!manager.is_subscribed(&b, &target_type));
        assert!(!manager.unsubscribe(&b, &target_type));
        assert!(manager.is_subscribed(&a, &target_type));
    }
}
