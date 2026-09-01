// ACTR: generated Rust workload entry scaffold
//! Entry point for the generated unified ACTR workload.
//!
//! Regenerate `src/generated/` with `actr gen -l rust -i proto`.

pub mod echo_service;
pub mod generated;

use actr_framework::entry;

// The type names feed `entry!` on wasm32/cdylib builds; on plain native
// builds the macro expands no branch that names them, so the imports sit
// unused there.
#[allow(unused_imports)]
use {
    echo_service::EchoServiceImpl, generated::actr_dispatch::ActrWorkload,
    generated::actr_handler::ActrHandler,
};

entry!(
    ActrWorkload<ActrHandler<EchoServiceImpl>>,
    ActrWorkload::new(ActrHandler::new(EchoServiceImpl::new()))
);
