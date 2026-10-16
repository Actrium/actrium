#![allow(clippy::empty_line_after_doc_comments)]

//! Cross-platform Actrium vault primitives.
//!
//! This crate owns vault cryptography, field serialization, and password
//! scoring. Platform shells still own OS credential stores, biometric prompts,
//! database handles, and UI state.

pub mod crypto;
mod ffi;
pub mod model;
pub mod password;

pub use ffi::*;

uniffi::include_scaffolding!("actrium_vault");
