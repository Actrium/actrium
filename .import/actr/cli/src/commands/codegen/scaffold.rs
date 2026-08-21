use crate::commands::SupportedLanguage;
use crate::commands::codegen::metadata::{ActrGenMetadata, TypeRef, load_required_metadata};
use crate::commands::codegen::traits::GenContext;
use crate::error::Result;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ScaffoldCatalog {
    pub local_services: Vec<ScaffoldService>,
    pub remote_services: Vec<ScaffoldService>,
}

#[derive(Debug, Clone)]
pub struct ScaffoldService {
    pub name: String,
    pub package: String,
    pub proto_file: PathBuf,
    pub handler_interface: Option<String>,
    pub workload_type: Option<String>,
    pub dispatcher_type: Option<String>,
    pub client_type: Option<String>,
    /// Manifest dependency alias for remote services (resolved at runtime
    /// via `Context::resolve_dependency`). `None` for local services or for
    /// pre-migration plugins that still carry `actr_type`.
    pub dependency_alias: Option<String>,
    /// Legacy actr_type for pre-migration plugins. New generators emit
    /// `dependency_alias` instead.
    pub actr_type: Option<String>,
    pub methods: Vec<ScaffoldMethod>,
}

#[derive(Debug, Clone)]
pub struct ScaffoldMethod {
    pub name: String,
    pub snake_name: String,
    pub input_type: String,
    pub output_type: String,
    pub route_key: String,
    pub input_ref: TypeRef,
    pub output_ref: TypeRef,
    /// Declared `actr.reply_mode` for this RPC (RFC-0323).
    pub reply_mode: crate::commands::codegen::dispatch_spec::ReplyMode,
}

impl ScaffoldCatalog {
    pub fn load(context: &GenContext, language: SupportedLanguage) -> Result<Self> {
        let metadata = load_required_metadata(&context.output, language)?;
        Ok(Self::from_metadata(&metadata))
    }

    pub(crate) fn from_metadata(metadata: &ActrGenMetadata) -> Self {
        Self {
            local_services: metadata
                .local_services
                .iter()
                .map(|service| ScaffoldService {
                    name: service.name.clone(),
                    package: service.package.clone(),
                    proto_file: PathBuf::from(&service.proto_file),
                    handler_interface: Some(service.handler_interface.clone()),
                    workload_type: non_empty_metadata_value(&service.workload_type),
                    dispatcher_type: non_empty_metadata_value(&service.dispatcher_type),
                    client_type: None,
                    dependency_alias: None,
                    actr_type: None,
                    methods: service
                        .methods
                        .iter()
                        .map(|method| ScaffoldMethod {
                            name: method.name.clone(),
                            snake_name: method.snake_name.clone(),
                            input_type: method.input_ref.type_name.clone(),
                            output_type: method.output_ref.type_name.clone(),
                            route_key: method.route_key.clone(),
                            input_ref: method.input_ref.clone(),
                            output_ref: method.output_ref.clone(),
                            reply_mode: method.reply_mode,
                        })
                        .collect(),
                })
                .collect(),
            remote_services: metadata
                .remote_services
                .iter()
                .map(|service| ScaffoldService {
                    name: service.name.clone(),
                    package: service.package.clone(),
                    proto_file: PathBuf::from(&service.proto_file),
                    handler_interface: None,
                    workload_type: None,
                    dispatcher_type: None,
                    client_type: Some(service.client_type.clone()),
                    dependency_alias: if service.dependency_alias.is_empty() {
                        None
                    } else {
                        Some(service.dependency_alias.clone())
                    },
                    actr_type: if service.actr_type.is_empty() {
                        None
                    } else {
                        Some(service.actr_type.clone())
                    },
                    methods: service
                        .methods
                        .iter()
                        .map(|method| ScaffoldMethod {
                            name: method.name.clone(),
                            snake_name: method.snake_name.clone(),
                            input_type: method.input_ref.type_name.clone(),
                            output_type: method.output_ref.type_name.clone(),
                            route_key: method.route_key.clone(),
                            input_ref: method.input_ref.clone(),
                            output_ref: method.output_ref.clone(),
                            reply_mode: method.reply_mode,
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    pub fn has_any_methods(&self) -> bool {
        self.local_services
            .iter()
            .any(|service| !service.methods.is_empty())
            || self
                .remote_services
                .iter()
                .any(|service| !service.methods.is_empty())
    }
}

fn non_empty_metadata_value(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

#[cfg(test)]
#[path = "scaffold_tests.rs"]
mod tests;
