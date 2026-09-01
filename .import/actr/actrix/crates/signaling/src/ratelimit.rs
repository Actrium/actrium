//! Signaling server rate limiting
//!
//! Implements dual rate limiting:
//! 1. **Connection rate limiting**: limits the rate of new WebSocket connections per IP
//! 2. **Message rate limiting**: limits the rate of messages sent per connection
//!
//! Implemented using the governor crate, configurable

use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use platform::config::signaling::{ConnectionRateLimit, MessageRateLimit};
use std::collections::HashMap;
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Connection rate limiter (per IP)
#[derive(Debug)]
pub struct ConnectionRateLimiter {
    /// Configuration
    config: ConnectionRateLimit,
    /// Per-IP rate limiter
    limiters: Arc<RwLock<HashMap<IpAddr, DefaultDirectRateLimiter>>>,
    /// Current connection count per IP
    connections: Arc<RwLock<HashMap<IpAddr, u32>>>,
}

impl ConnectionRateLimiter {
    /// Create a new connection rate limiter
    pub fn new(config: ConnectionRateLimit) -> Self {
        Self {
            config,
            limiters: Arc::new(RwLock::new(HashMap::new())),
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check whether a new connection is allowed
    ///
    /// Returns Ok(()) if allowed, Err otherwise
    pub async fn check_connection(&self, ip: IpAddr) -> Result<(), String> {
        if !self.config.enabled {
            return Ok(());
        }

        // Check concurrent connection count
        let connections = self.connections.read().await;
        if let Some(&count) = connections.get(&ip)
            && count >= self.config.max_concurrent_per_ip
        {
            platform::recording::warn!(
                "IP {} exceeded max concurrent connections: {}/{}",
                ip,
                count,
                self.config.max_concurrent_per_ip
            );
            return Err(format!(
                "Too many concurrent connections from your IP: {}/{}",
                count, self.config.max_concurrent_per_ip
            ));
        }
        drop(connections);

        // Check connection rate
        let mut limiters = self.limiters.write().await;
        let limiter = limiters.entry(ip).or_insert_with(|| {
            // per_minute connections per minute, convert to per second
            let per_second =
                NonZeroU32::new((self.config.per_minute as f64 / 60.0).ceil().max(1.0) as u32)
                    .unwrap();

            let quota = Quota::per_second(per_second)
                .allow_burst(NonZeroU32::new(self.config.burst_size).unwrap());

            RateLimiter::direct(quota)
        });

        match limiter.check() {
            Ok(_) => {
                platform::recording::debug!("IP {} passed connection rate limit check", ip);
                Ok(())
            }
            Err(_) => {
                platform::recording::warn!("IP {} exceeded connection rate limit", ip);
                Err(format!(
                    "Too many connection attempts. Limit: {} connections/minute",
                    self.config.per_minute
                ))
            }
        }
    }

    /// Increment connection count
    pub async fn increment_connection(&self, ip: IpAddr) {
        if !self.config.enabled {
            return;
        }

        let mut connections = self.connections.write().await;
        *connections.entry(ip).or_insert(0) += 1;
        platform::recording::debug!(
            "IP {} connection count: {}",
            ip,
            connections.get(&ip).unwrap()
        );
    }

    /// Decrement connection count
    pub async fn decrement_connection(&self, ip: IpAddr) {
        if !self.config.enabled {
            return;
        }

        let mut connections = self.connections.write().await;
        if let Some(count) = connections.get_mut(&ip) {
            *count = count.saturating_sub(1);
            platform::recording::debug!("IP {} connection count decreased to: {}", ip, count);

            // If connection count is 0, remove the entry to save memory
            if *count == 0 {
                connections.remove(&ip);
            }
        }
    }

    /// Get statistics
    pub async fn stats(&self) -> (usize, usize) {
        let limiters = self.limiters.read().await;
        let connections = self.connections.read().await;
        (limiters.len(), connections.len())
    }
}

/// Message rate limiter (per connection ID)
#[derive(Debug)]
pub struct MessageRateLimiter {
    /// Configuration
    config: MessageRateLimit,
    /// Per-connection rate limiter
    limiters: Arc<RwLock<HashMap<String, DefaultDirectRateLimiter>>>,
}

impl MessageRateLimiter {
    /// Create a new message rate limiter
    pub fn new(config: MessageRateLimit) -> Self {
        Self {
            config,
            limiters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check whether sending a message is allowed
    ///
    /// Returns Ok(()) if allowed, Err otherwise
    pub async fn check_message(&self, connection_id: &str) -> Result<(), String> {
        if !self.config.enabled {
            return Ok(());
        }

        let mut limiters = self.limiters.write().await;
        let limiter = limiters
            .entry(connection_id.to_string())
            .or_insert_with(|| {
                let per_second = NonZeroU32::new(self.config.per_second).unwrap();
                let quota = Quota::per_second(per_second)
                    .allow_burst(NonZeroU32::new(self.config.burst_size).unwrap());

                RateLimiter::direct(quota)
            });

        match limiter.check() {
            Ok(_) => {
                platform::recording::debug!(
                    "Connection {} passed message rate limit check",
                    connection_id
                );
                Ok(())
            }
            Err(_) => {
                platform::recording::warn!(
                    "Connection {} exceeded message rate limit",
                    connection_id
                );
                Err(format!(
                    "Too many messages. Limit: {} messages/second",
                    self.config.per_second
                ))
            }
        }
    }

    /// Remove the rate limiter for a connection (called on connection close)
    pub async fn remove_connection(&self, connection_id: &str) {
        if !self.config.enabled {
            return;
        }

        let mut limiters = self.limiters.write().await;
        limiters.remove(connection_id);
        platform::recording::debug!("Removed rate limiter for connection {}", connection_id);
    }

    /// Get statistics
    pub async fn stats(&self) -> usize {
        let limiters = self.limiters.read().await;
        limiters.len()
    }
}

/// Per-target aggregate rate/byte limiter (across all senders, issue #351)
///
/// The existing [`MessageRateLimiter`] rate-limits per *sender* connection; this limiter aggregates per *receiver* target
/// to prevent multiple individually-compliant senders from overwhelming a single target together. Each target connection maintains a
/// message token bucket (messages/sec) and a byte token bucket (bytes/sec).
#[derive(Debug)]
pub struct TargetRateLimiter {
    /// Per-target aggregate message rate (messages/sec)
    msg_per_second: u32,
    /// Per-target aggregate byte budget (bytes/sec)
    bytes_per_second: u32,
    /// Per-target-connection (message, byte) limiters
    limiters: Arc<RwLock<HashMap<String, TargetLimiters>>>,
}

#[derive(Debug)]
struct TargetLimiters {
    messages: DefaultDirectRateLimiter,
    bytes: DefaultDirectRateLimiter,
}

impl TargetRateLimiter {
    /// Create a new per-target aggregate limiter
    pub fn new(msg_per_second: u32, bytes_per_second: u32) -> Self {
        Self {
            msg_per_second: msg_per_second.max(1),
            bytes_per_second: bytes_per_second.max(1),
            limiters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check whether a message of `byte_len` bytes may be delivered to the target
    ///
    /// Returns Ok(()) if allowed (both buckets pass), otherwise Err (aggregate budget exceeded;
    /// the caller should drop the message and count it, not block or queue indefinitely).
    pub async fn check_target(&self, target_id: &str, byte_len: u32) -> Result<(), String> {
        let mut limiters = self.limiters.write().await;
        let entry = limiters.entry(target_id.to_string()).or_insert_with(|| {
            let msg_quota =
                Quota::per_second(NonZeroU32::new(self.msg_per_second).unwrap_or(NonZeroU32::MIN))
                    .allow_burst(NonZeroU32::new(self.msg_per_second).unwrap_or(NonZeroU32::MIN));
            let byte_quota = Quota::per_second(
                NonZeroU32::new(self.bytes_per_second).unwrap_or(NonZeroU32::MIN),
            )
            .allow_burst(NonZeroU32::new(self.bytes_per_second).unwrap_or(NonZeroU32::MIN));
            TargetLimiters {
                messages: RateLimiter::direct(msg_quota),
                bytes: RateLimiter::direct(byte_quota),
            }
        });

        // Message-count check (1 token per message)
        entry.messages.check().map_err(|_| {
            format!(
                "Target {target_id} exceeded aggregate message rate: {} msg/s",
                self.msg_per_second
            )
        })?;

        // Byte check (byte_len tokens)
        let tokens = NonZeroU32::new(byte_len.max(1)).unwrap_or(NonZeroU32::MIN);
        match entry.bytes.check_n(tokens) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(format!(
                "Target {target_id} single message of {byte_len} bytes exceeds aggregate byte capacity ({} B/s)",
                self.bytes_per_second
            )),
            Err(_) => Err(format!(
                "Target {target_id} exceeded aggregate byte budget: {} B/s",
                self.bytes_per_second
            )),
        }
    }

    /// Remove the target's limiter (called when the connection closes)
    pub async fn remove_target(&self, target_id: &str) {
        let mut limiters = self.limiters.write().await;
        limiters.remove(target_id);
        platform::recording::debug!("Removed target rate limiter for {}", target_id);
    }

    /// Get statistics (number of tracked targets)
    pub async fn stats(&self) -> usize {
        let limiters = self.limiters.read().await;
        limiters.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[tokio::test]
    async fn test_connection_rate_limiter_creation() {
        let config = ConnectionRateLimit::default();
        let _limiter = ConnectionRateLimiter::new(config);
    }

    #[tokio::test]
    async fn test_message_rate_limiter_creation() {
        let config = MessageRateLimit::default();
        let _limiter = MessageRateLimiter::new(config);
    }

    #[tokio::test]
    async fn test_connection_increment_decrement() {
        let config = ConnectionRateLimit::default();
        let limiter = ConnectionRateLimiter::new(config);
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        limiter.increment_connection(ip).await;
        let (_, conn_count) = limiter.stats().await;
        assert_eq!(conn_count, 1);

        limiter.decrement_connection(ip).await;
        let (_, conn_count) = limiter.stats().await;
        assert_eq!(conn_count, 0);
    }

    #[tokio::test]
    async fn test_message_limiter_removal() {
        let config = MessageRateLimit::default();
        let limiter = MessageRateLimiter::new(config);
        let conn_id = "test-connection-1";

        // Send a message to create the limiter
        let _ = limiter.check_message(conn_id).await;
        assert_eq!(limiter.stats().await, 1);

        // Remove the connection
        limiter.remove_connection(conn_id).await;
        assert_eq!(limiter.stats().await, 0);
    }
}
