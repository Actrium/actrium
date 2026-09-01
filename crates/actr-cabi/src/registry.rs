//! Workload registry — compile-time registration via `inventory`.
//!
//! `export_workload!(MyWorkload, "cabi.Echo")` submits a `WorkloadEntry` whose
//! `link_fn` captures the concrete workload type and calls the public
//! `Node::link(node, workload)` (`core/hyper/src/lib.rs:1034`). The internal
//! `LinkedWorkloadHandle` is `pub(crate)`, so actr-cabi cannot use
//! `link_handle`; the macro closes over the concrete type in the workload
//! crate, where the type is known.

use std::future::Future;
use std::pin::Pin;

use actr_hyper::{Attached, HyperError, Init, Node};

/// Function that consumes a `Node<Init>`, instantiates the registered workload,
/// links it, and returns the `Node<Attached>` (or error).
pub type LinkFn =
    fn(
        Node<Init>,
    ) -> Pin<Box<dyn Future<Output = Result<Node<Attached>, HyperError>> + Send + 'static>>;

/// A registered workload. Submitted at link time by `export_workload!`.
pub struct WorkloadEntry {
    pub name: &'static str,
    pub link_fn: LinkFn,
}

inventory::collect!(WorkloadEntry);

/// Look up a registered workload by name.
pub fn find_workload(name: &str) -> Option<&'static WorkloadEntry> {
    inventory::iter::<WorkloadEntry>().find(|e| e.name == name)
}

// ── bundle identity ─────────────────────────────────────────────────────────

/// A bundle ID declared by a workload cdylib via `export_bundle!`.
///
/// At most one bundle ID is expected per process: a single `actr_cabi`
/// cdylib statically links one Rust workload (or a static combination of
/// several). The host reads it to confirm which native provider was loaded.
pub struct BundleEntry {
    pub id: &'static str,
}

inventory::collect!(BundleEntry);

/// Return the first registered bundle ID, or `None` if no workload cdylib
/// declared one.
pub fn bundle_id() -> Option<&'static str> {
    inventory::iter::<BundleEntry>().next().map(|e| e.id)
}

/// Return the number of registered bundle IDs. Should be 0 or 1; a value
/// greater than 1 signals multiple providers were linked, which violates the
/// single-provider contract.
pub fn bundle_count() -> usize {
    inventory::iter::<BundleEntry>().count()
}

/// Register a Rust workload under a string name.
///
/// The workload type must implement `actr_framework::Workload` + `Default`.
/// The concrete type is captured here (in the workload crate) so the public
/// `Node::link` API is used without needing access to `pub(crate)`
/// `LinkedWorkloadHandle`.
///
/// ```rust,ignore
/// pub struct MyWorkload;
/// impl actr_framework::Workload for MyWorkload { type Dispatcher = MyDispatcher; }
/// actr_cabi::export_workload!(MyWorkload, "cabi.MyOp");
/// ```
#[macro_export]
macro_rules! export_workload {
    ($workload_type:ty, $name:expr) => {
        $crate::inventory::submit! {
            $crate::WorkloadEntry {
                name: $name,
                link_fn: |node| Box::pin(async move {
                    let workload: $workload_type = ::core::default::Default::default();
                    ::actr_hyper::Node::link(node, workload).await
                }),
            }
        }
    };
}

/// Declare the bundle ID for this workload cdylib.
///
/// Call this exactly once per workload crate (alongside `export_workload!`).
/// The host's native validation surfaces the ID so a mislinked `actr_cabi`
/// can be identified before driving the typestate chain. Multiple workloads
/// that are statically combined into one cdylib share a single bundle ID.
///
/// ```rust,ignore
/// actr_cabi::export_bundle!("com.example.myworkload");
/// ```
#[macro_export]
macro_rules! export_bundle {
    ($id:expr) => {
        $crate::inventory::submit! {
            $crate::BundleEntry { id: $id }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_workload_is_none() {
        assert!(find_workload("cabi.does-not-exist").is_none());
    }
}
