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
//! use actr_protocol::{ActrId, ActrType, Realm};
//!
//! let mut manager = PresenceManager::new();
//!
//! let actor_a_id = ActrId { /* ... */ };
//! let user_service_type = ActrType {
//!     publisher: "acme".to_string(),
//!     name: "user-service".to_string(),
//!     version: "1.0.0".to_string(),
//! };
//!
//! // Actor A subscribes to online events for the user-service type
//! manager.subscribe(actor_a_id, user_service_type.clone());
//!
//! // When a new user-service instance registers
//! let subscribers = manager.get_subscribers(&user_service_type);
//! // Push ActrUpEvent to subscribers
//! ```

use actr_protocol::{ActrId, ActrType};
use platform::RealmError;
use platform::realm::acl::ActorAcl;
use std::collections::HashMap;

/// Presence subscription manager
#[derive(Debug, Default)]
pub struct PresenceManager {
    /// Subscription map: target_type -> Vec<subscriber_actor_id>
    ///
    /// Key: the subscribed service type (ActrType)
    /// Value: list of Actors subscribed to that type
    subscriptions: HashMap<ActrType, Vec<ActrId>>,
}

impl PresenceManager {
    /// Create a new PresenceManager
    pub fn new() -> Self {
        Self {
            subscriptions: HashMap::new(),
        }
    }

    /// Subscribe to Actor online events for a specific type
    ///
    /// # Arguments
    /// - `subscriber`: the subscriber's ActrId
    /// - `target_type`: the service type to subscribe to
    ///
    /// # Example
    /// ```ignore
    /// manager.subscribe(client_actor_id, user_service_type);
    /// ```
    pub fn subscribe(&mut self, subscriber: ActrId, target_type: ActrType) {
        platform::recording::info!(
            "Actor {} subscribed to {}/{} online events",
            subscriber.to_string_repr(),
            target_type.publisher,
            target_type.name
        );

        let subscribers = self.subscriptions.entry(target_type).or_default();

        // Avoid duplicate subscriptions
        if !subscribers.iter().any(|id| id == &subscriber) {
            subscribers.push(subscriber);
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

        if let Some(subscribers) = self.subscriptions.get_mut(target_type) {
            let original_len = subscribers.len();
            subscribers.retain(|id| id != subscriber);

            let removed = subscribers.len() < original_len;
            if removed {
                platform::recording::debug!(
                    "Unsubscribe successful, remaining subscriber count: {}",
                    subscribers.len()
                );

                // If no subscribers remain, remove the entire entry
                if subscribers.is_empty() {
                    self.subscriptions.remove(target_type);
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
        self.subscriptions.retain(|_target_type, subscribers| {
            let original_len = subscribers.len();
            subscribers.retain(|id| id != subscriber);
            removed_count += original_len - subscribers.len();

            // If the list is empty, return false to remove the entry
            !subscribers.is_empty()
        });

        if removed_count > 0 {
            platform::recording::info!("Cleared {} subscriptions", removed_count);
        }

        removed_count
    }

    /// Get all Actors subscribed to a specific type
    ///
    /// # Arguments
    /// - `target_type`: the service type
    ///
    /// # Returns
    /// A list of subscriber ActrIds (references)
    ///
    /// # Usage
    /// When a new target_type instance registers, use this method to get
    /// all subscribers that need to be notified
    pub fn get_subscribers(&self, target_type: &ActrType) -> Vec<&ActrId> {
        self.subscriptions
            .get(target_type)
            .map(|subscribers| subscribers.iter().collect())
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
            .get(target_type)
            .map(|subscribers| subscribers.iter().any(|id| id == subscriber))
            .unwrap_or(false)
    }

    /// Get subscribers with ACL filtering
    ///
    /// Returns only subscribers that are allowed to discover the target actor
    /// based on ACL rules
    ///
    /// # Arguments
    ///
    /// - `target_actor_id`: The actor that came online
    ///
    /// # Returns
    ///
    /// List of subscriber ActorIds that passed ACL checks
    pub async fn get_subscribers_with_acl(&self, target_actor_id: &ActrId) -> Vec<ActrId> {
        let target_type = &target_actor_id.r#type;
        let subscribers = self.get_subscribers(target_type);
        let total_count = subscribers.len(); // Save count before moving

        let mut allowed_subscribers = Vec::new();

        for subscriber_id in subscribers {
            // ACL check: can subscriber discover target?
            match Self::check_discovery_acl(subscriber_id, target_actor_id).await {
                Ok(true) => {
                    allowed_subscribers.push(subscriber_id.clone());
                }
                Ok(false) => {
                    platform::recording::debug!(
                        "ACL denied discovery notification: subscriber={}, target={}",
                        subscriber_id.to_string_repr(),
                        target_actor_id.to_string_repr()
                    );
                }
                Err(e) => {
                    platform::recording::warn!(
                        "ACL check failed, denying notification: subscriber={}, target={}, error={}",
                        subscriber_id.to_string_repr(),
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
    /// - `from_actor`: Subscriber actor ID
    /// - `to_actor`: Target actor ID
    ///
    /// # Returns
    ///
    /// Returns true if discovery is allowed based on ACL rules
    async fn check_discovery_acl(
        from_actor: &ActrId,
        to_actor: &ActrId,
    ) -> Result<bool, RealmError> {
        // Extract realm and actor types
        let from_realm = from_actor.realm.realm_id;
        let to_realm = to_actor.realm.realm_id;

        let from_type = from_actor.r#type.to_string_repr();
        let to_type = to_actor.r#type.to_string_repr();

        ActorAcl::can_discover(from_realm, to_realm, &from_type, &to_type).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actr_protocol::Realm;

    fn create_test_actor_id(serial: u64) -> ActrId {
        ActrId {
            serial_number: serial,
            r#type: ActrType {
                publisher: "test".to_string(),
                name: "test-actor".to_string(),
                version: "1.0.0".to_string(),
            },
            realm: Realm { realm_id: 0 },
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

        manager.subscribe(actor1.clone(), target_type.clone());
        manager.subscribe(actor2.clone(), target_type.clone());

        let subscribers = manager.get_subscribers(&target_type);
        assert_eq!(subscribers.len(), 2);
    }

    #[test]
    fn test_unsubscribe() {
        let mut manager = PresenceManager::new();
        let actor1 = create_test_actor_id(1);
        let target_type = create_test_actor_type("user-service");

        manager.subscribe(actor1.clone(), target_type.clone());
        assert!(manager.unsubscribe(&actor1, &target_type));

        let subscribers = manager.get_subscribers(&target_type);
        assert_eq!(subscribers.len(), 0);
    }

    #[test]
    fn test_unsubscribe_all() {
        let mut manager = PresenceManager::new();
        let actor1 = create_test_actor_id(1);
        let type1 = create_test_actor_type("user-service");
        let type2 = create_test_actor_type("order-service");

        manager.subscribe(actor1.clone(), type1.clone());
        manager.subscribe(actor1.clone(), type2.clone());

        let removed = manager.unsubscribe_all(&actor1);
        assert_eq!(removed, 2);

        assert_eq!(manager.get_subscribers(&type1).len(), 0);
        assert_eq!(manager.get_subscribers(&type2).len(), 0);
    }

    #[test]
    fn test_duplicate_subscription() {
        let mut manager = PresenceManager::new();
        let actor1 = create_test_actor_id(1);
        let target_type = create_test_actor_type("user-service");

        manager.subscribe(actor1.clone(), target_type.clone());
        manager.subscribe(actor1.clone(), target_type.clone()); // duplicate subscription

        let subscribers = manager.get_subscribers(&target_type);
        assert_eq!(subscribers.len(), 1); // should only have one
    }

    #[test]
    fn test_stats() {
        let mut manager = PresenceManager::new();
        let actor1 = create_test_actor_id(1);
        let actor2 = create_test_actor_id(2);
        let type1 = create_test_actor_type("user-service");
        let type2 = create_test_actor_type("order-service");

        manager.subscribe(actor1.clone(), type1.clone());
        manager.subscribe(actor2.clone(), type1.clone());
        manager.subscribe(actor1.clone(), type2.clone());

        let (type_count, subscriber_count) = manager.stats();
        assert_eq!(type_count, 2); // 2 types
        assert_eq!(subscriber_count, 3); // 3 subscriptions
    }
}
