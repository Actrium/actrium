//! Outbound network access.

pub mod fetch;

pub use fetch::{CachedFetcher, FetchError, fetch_public_json, is_forbidden_address};
