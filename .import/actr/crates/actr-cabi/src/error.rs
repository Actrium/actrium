//! Error codes and C-side memory allocation helpers for actr-cabi.
//!
//! Callback buffers are valid only for the duration of the callback. A typed
//! allocation registry owns them so an arbitrary pointer, mismatched length,
//! wrong free function, or repeated free cannot reach the Rust allocator.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

// ── Status codes ────────────────────────────────────────────────────────────

pub const SUCCESS: i32 = 0;
pub const RT_INIT_FAILED: i32 = -1;
pub const RT_NOT_INITIALIZED: i32 = -2;
pub const NULL_POINTER: i32 = -10;
pub const INVALID_UTF8: i32 = -11;
pub const WORKLOAD_NOT_FOUND: i32 = -20;
pub const HANDLE_INVALID: i32 = -21;
/// `actr_host_respond_reverse_rpc` was called with a request_id that is
/// unknown, already answered, or past its timeout.
pub const REQUEST_ID_UNKNOWN: i32 = -22;
pub const PANIC: i32 = -99;

// ── Validated callback-buffer allocations ──────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum AllocationKind {
    Bytes,
    CString,
}

struct Allocation {
    // Keeping the box in the registry owns the exact allocation and its
    // allocator provenance. Removing the entry drops it with the right layout.
    _bytes: Box<[u8]>,
    len: usize,
    kind: AllocationKind,
}

static ALLOCATIONS: OnceLock<Mutex<HashMap<usize, Allocation>>> = OnceLock::new();

fn allocations() -> MutexGuard<'static, HashMap<usize, Allocation>> {
    ALLOCATIONS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn register(mut bytes: Box<[u8]>, kind: AllocationKind) -> *mut u8 {
    let ptr = bytes.as_mut_ptr();
    let key = ptr as usize;
    let len = bytes.len();
    let mut allocations = allocations();
    if allocations.try_reserve(1).is_err() || allocations.contains_key(&key) {
        return std::ptr::null_mut();
    }
    allocations.insert(
        key,
        Allocation {
            _bytes: bytes,
            len,
            kind,
        },
    );
    ptr
}

fn unregister(ptr: *mut u8, len: usize, kind: AllocationKind) {
    if ptr.is_null() || len == 0 {
        return;
    }
    let key = ptr as usize;
    let removed = {
        let mut allocations = allocations();
        match allocations.get(&key) {
            Some(allocation) if allocation.len == len && allocation.kind == kind => {
                allocations.remove(&key)
            }
            _ => None,
        }
    };
    drop(removed);
}

/// Allocate a NUL-terminated buffer and register its exact ownership metadata.
/// Returns null on a recoverable allocation failure.
pub fn alloc_cstring(value: &str) -> *mut u8 {
    let Some(len) = value.len().checked_add(1) else {
        return std::ptr::null_mut();
    };
    let mut bytes = Vec::new();
    if bytes.try_reserve_exact(len).is_err() {
        return std::ptr::null_mut();
    }
    bytes.extend_from_slice(value.as_bytes());
    bytes.push(0);
    register(bytes.into_boxed_slice(), AllocationKind::CString)
}

/// Allocate a byte buffer and register its exact ownership metadata.
/// Returns `(null, 0)` for an empty input or on a recoverable allocation
/// failure.
pub fn alloc_bytes(value: &[u8]) -> (*mut u8, usize) {
    if value.is_empty() {
        return (std::ptr::null_mut(), 0);
    }
    let mut bytes = Vec::new();
    if bytes.try_reserve_exact(value.len()).is_err() {
        return (std::ptr::null_mut(), 0);
    }
    bytes.extend_from_slice(value);
    let len = bytes.len();
    let ptr = register(bytes.into_boxed_slice(), AllocationKind::Bytes);
    if ptr.is_null() {
        (std::ptr::null_mut(), 0)
    } else {
        (ptr, len)
    }
}

/// Release a registered C string allocation.
///
/// Invalid, mismatched, and already-freed inputs are safe no-ops. `len`
/// includes the trailing NUL byte.
pub fn free_cstring(ptr: *mut u8, len: usize) {
    unregister(ptr, len, AllocationKind::CString);
}

/// Release a registered byte-buffer allocation.
///
/// Invalid, mismatched, and already-freed inputs are safe no-ops.
pub fn free_bytes(ptr: *mut u8, len: usize) {
    unregister(ptr, len, AllocationKind::Bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_wrong_free_kind_length_and_double_free() {
        let bytes = alloc_bytes(b"abc");
        assert!(!bytes.0.is_null());

        free_cstring(bytes.0, bytes.1);
        free_bytes(bytes.0, bytes.1 + 1);
        free_bytes(bytes.0, bytes.1);
        free_bytes(bytes.0, bytes.1);
    }
}
