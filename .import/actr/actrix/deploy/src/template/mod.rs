//! Template processing module

mod processor;
mod systemd_service;

pub use processor::TemplateProcessor;
pub use systemd_service::SystemdServiceTemplate;
