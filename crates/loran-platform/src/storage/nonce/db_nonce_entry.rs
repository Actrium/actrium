//! Database nonce entry
//!
//! Defines the nonce entry structure stored in the database

use serde::{Deserialize, Serialize};

/// Nonce entry stored in the database
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct DbNonceEntry {
    pub id: Option<i64>,
    pub nonce: String,
    pub context: Option<String>,
    pub expires_at: i64, // Unix timestamp
    pub created_at: i64, // Unix timestamp
}
