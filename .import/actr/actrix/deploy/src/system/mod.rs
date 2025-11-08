//! System utilities module

mod check_result;
mod dependencies;
mod helpers;
mod install;
mod network;
mod provider;
pub mod providers;
mod uninstall;
mod user_management;
mod validation;

// Public exports
pub use check_result::DependencyCheckResult;
pub use dependencies::{check_dependencies, check_dependencies_data};
pub use helpers::{clear_input_buffer, press_any_key_to_with_interrupt};
pub use install::{install_application, install_systemd_service};
pub use network::NetworkUtils;
pub use provider::{SystemProvider, SystemProviderFactory};
pub use uninstall::uninstall_application;
// pub use user_management::{ensure_group_exists, ensure_user_exists};
pub use validation::{validate_port, validate_username};
