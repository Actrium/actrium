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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_workload_is_none() {
        assert!(find_workload("cabi.does-not-exist").is_none());
    }
}
