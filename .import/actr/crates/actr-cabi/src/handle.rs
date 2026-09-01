//! Opaque C handles for the actr typestate chain.
//!
//! Handles are non-zero integer tokens encoded as `*mut c_void`. The pointed-to
//! address is never dereferenced. A process-global, typed registry owns each
//! value, which lets the C boundary reject stale, duplicated, and wrong-state
//! handles without reading freed memory. State transitions atomically remove
//! their input token, so a consumed token can never become valid again.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Mutex, MutexGuard, OnceLock};

use actr_hyper::{ActrRef, Attached, Init, Node, Registered};

use crate::registry::LinkFn;

// ── Public opaque handle typedefs ───────────────────────────────────────────

pub type ActrNodeInitHandle = *mut c_void;
pub type ActrNodeAttachedHandle = *mut c_void;
pub type ActrNodeRegisteredHandle = *mut c_void;
pub type ActrRefHandle = *mut c_void;
pub type WorkloadHandle = *mut c_void;

// ── Typed handle registry ───────────────────────────────────────────────────

enum HandleValue {
    Init(Node<Init>),
    Attached(Node<Attached>),
    Registered(Node<Registered>),
    ActrRef(ActrRef),
    Workload(LinkFn),
}

struct HandleTable {
    next: usize,
    values: HashMap<usize, HandleValue>,
}

impl Default for HandleTable {
    fn default() -> Self {
        Self {
            next: 1,
            values: HashMap::new(),
        }
    }
}

impl HandleTable {
    fn insert(&mut self, value: HandleValue) -> *mut c_void {
        let id = loop {
            let candidate = self.next;
            self.next = self.next.checked_add(1).unwrap_or(1);
            if candidate != 0 && !self.values.contains_key(&candidate) {
                break candidate;
            }
        };
        self.values.insert(id, value);
        id as *mut c_void
    }
}

static HANDLE_TABLE: OnceLock<Mutex<HandleTable>> = OnceLock::new();

fn table() -> MutexGuard<'static, HandleTable> {
    HANDLE_TABLE
        .get_or_init(|| Mutex::new(HandleTable::default()))
        .lock()
        // A panic while holding the registry must not permanently disable the
        // FFI boundary. All mutations preserve HashMap validity.
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn handle_id(handle: *mut c_void) -> Option<usize> {
    let id = handle as usize;
    (id != 0).then_some(id)
}

fn insert(value: HandleValue) -> *mut c_void {
    table().insert(value)
}

fn take_matching(
    handle: *mut c_void,
    matches: impl FnOnce(&HandleValue) -> bool,
) -> Option<HandleValue> {
    let id = handle_id(handle)?;
    let mut table = table();
    if !table.values.get(&id).is_some_and(matches) {
        return None;
    }
    table.values.remove(&id)
}

// ── Node<Init> ──────────────────────────────────────────────────────────────

pub(crate) fn box_init(node: Node<Init>) -> ActrNodeInitHandle {
    insert(HandleValue::Init(node))
}

pub(crate) fn take_init(handle: ActrNodeInitHandle) -> Option<Node<Init>> {
    match take_matching(handle, |value| matches!(value, HandleValue::Init(_)))? {
        HandleValue::Init(node) => Some(node),
        _ => unreachable!("handle kind was checked while holding the registry lock"),
    }
}

// ── Node<Attached> ──────────────────────────────────────────────────────────

pub(crate) fn box_attached(node: Node<Attached>) -> ActrNodeAttachedHandle {
    insert(HandleValue::Attached(node))
}

pub(crate) fn take_attached(handle: ActrNodeAttachedHandle) -> Option<Node<Attached>> {
    match take_matching(handle, |value| matches!(value, HandleValue::Attached(_)))? {
        HandleValue::Attached(node) => Some(node),
        _ => unreachable!("handle kind was checked while holding the registry lock"),
    }
}

// ── Node<Registered> ────────────────────────────────────────────────────────

pub(crate) fn box_registered(node: Node<Registered>) -> ActrNodeRegisteredHandle {
    insert(HandleValue::Registered(node))
}

pub(crate) fn take_registered(handle: ActrNodeRegisteredHandle) -> Option<Node<Registered>> {
    match take_matching(handle, |value| matches!(value, HandleValue::Registered(_)))? {
        HandleValue::Registered(node) => Some(node),
        _ => unreachable!("handle kind was checked while holding the registry lock"),
    }
}

// ── ActrRef ─────────────────────────────────────────────────────────────────

pub(crate) fn box_actr_ref(actr_ref: ActrRef) -> ActrRefHandle {
    insert(HandleValue::ActrRef(actr_ref))
}

/// Clone an `ActrRef` while holding the registry lock. Releasing the token from
/// another thread is serialized with this clone, so the inner value cannot be
/// read after it is dropped.
pub(crate) fn clone_actr_ref(handle: ActrRefHandle) -> Option<ActrRef> {
    let id = handle_id(handle)?;
    match table().values.get(&id) {
        Some(HandleValue::ActrRef(actr_ref)) => Some(actr_ref.clone()),
        _ => None,
    }
}

/// Remove and drop an `ActrRef` token. Wrong-state tokens are left untouched.
pub(crate) fn drop_actr_ref(handle: ActrRefHandle) -> bool {
    take_matching(handle, |value| matches!(value, HandleValue::ActrRef(_))).is_some()
}

// ── Workload ────────────────────────────────────────────────────────────────

pub(crate) fn box_workload(link_fn: LinkFn) -> WorkloadHandle {
    insert(HandleValue::Workload(link_fn))
}

/// Atomically consume the two differently typed inputs to `actr_node_link`.
/// If either token is invalid, stale, aliased, or has the wrong type, neither
/// token is consumed.
pub(crate) fn take_link_inputs(
    init_handle: ActrNodeInitHandle,
    workload_handle: WorkloadHandle,
) -> Option<(Node<Init>, LinkFn)> {
    let init_id = handle_id(init_handle)?;
    let workload_id = handle_id(workload_handle)?;
    if init_id == workload_id {
        return None;
    }

    let mut table = table();
    if !matches!(table.values.get(&init_id), Some(HandleValue::Init(_)))
        || !matches!(
            table.values.get(&workload_id),
            Some(HandleValue::Workload(_))
        )
    {
        return None;
    }

    let node = match table.values.remove(&init_id) {
        Some(HandleValue::Init(node)) => node,
        _ => unreachable!("init handle kind was checked while holding the registry lock"),
    };
    let link_fn = match table.values.remove(&workload_id) {
        Some(HandleValue::Workload(link_fn)) => link_fn,
        _ => unreachable!("workload handle kind was checked while holding the registry lock"),
    };
    Some((node, link_fn))
}

/// Remove and correctly drop any handle kind. Returns `false` for null, stale,
/// or unknown tokens, making repeated release calls harmless.
pub(crate) fn drop_handle(handle: *mut c_void) -> bool {
    let Some(id) = handle_id(handle) else {
        return false;
    };
    let removed = {
        let mut table = table();
        table.values.remove(&id)
    };
    // Drop the typed value only after releasing the registry lock. A value's
    // destructor may transitively invoke host code that re-enters actr-cabi.
    removed.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::future::Future;
    use std::pin::Pin;

    fn dummy_link(
        node: Node<Init>,
    ) -> Pin<
        Box<dyn Future<Output = Result<Node<Attached>, actr_hyper::HyperError>> + Send + 'static>,
    > {
        Box::pin(async move {
            drop(node);
            unreachable!("the registry test never invokes its workload link function")
        })
    }

    #[test]
    fn consumed_and_released_tokens_never_validate_twice() {
        let first = box_workload(dummy_link);
        assert!(matches!(
            take_matching(first, |value| matches!(value, HandleValue::Workload(_))),
            Some(HandleValue::Workload(_))
        ));
        assert!(take_matching(first, |_| true).is_none());

        let second = box_workload(dummy_link);
        assert!(drop_handle(second));
        assert!(!drop_handle(second));
    }
}
