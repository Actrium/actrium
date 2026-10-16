#![allow(clippy::empty_line_after_doc_comments)]

//! Cross-platform Actrium domain and contract primitives.
//!
//! This crate intentionally avoids UI and platform APIs. Swift/Kotlin/Windows
//! layers should treat it as the canonical source for shared data shapes,
//! package manifest validation, and native bridge payload contracts.

pub mod actr_config;
pub mod actr_package;
pub mod auth;
pub mod bridge;
pub mod browser;
pub mod browser_data;
pub mod c_api;
pub mod domain;
mod ffi;
pub mod persistence;
pub mod runtime_persistence;
pub mod speech;

pub use ffi::*;

uniffi::include_scaffolding!("actrium_core");

pub const PRODUCT_NAME: &str = "Actrium";

#[cfg(test)]
mod tests {
    #[test]
    fn product_name_is_stable() {
        assert_eq!(super::PRODUCT_NAME, "Actrium");
    }
}
