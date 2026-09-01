//! Service status definitions
//!
//! Defines the service state enum used to represent the running state of various services

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServiceState {
    Unknown,
    Running(String),
    Error(String),
}
