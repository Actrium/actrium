/*
 * keel_guest.h — C guest SDK for keel native (non-JS) WASM guests.
 *
 * Proves the linear-memory host:* ABI (docs/native-guest.zh.md) is genuinely
 * language-neutral: a guest compiled from C binds the SAME imports/exports the
 * Rust SDK (crates/keel-native-guest) uses, and the Phase A NativeGuestHost loads it
 * unchanged.
 *
 * This first cut covers the one-way RENDER path (sync, no host round-trip).
 * The async host:* call import (keel_host_call) is declared for reference
 * only — the callback-resolve executor around it, and the guest-side event
 * export (keel_on_event), are NOT provided here; a full async/event runtime
 * in C is a follow-up. See the Rust SDK (crates/keel-native-guest) for the reference
 * implementation of both.
 */
#ifndef KEEL_GUEST_H
#define KEEL_GUEST_H

typedef unsigned int keel_u32; /* wasm32: 32-bit */

/* ---- ABI imports (host provides, module "env") ---- */
__attribute__((import_module("env"), import_name("keel_send_batch"))) extern void
keel_send_batch(const char *batch_ptr, keel_u32 batch_len);

__attribute__((import_module("env"), import_name("keel_host_call"))) extern void
keel_host_call(const char *mod_ptr, keel_u32 mod_len, const char *method_ptr,
               keel_u32 method_len, const char *in_ptr, keel_u32 in_len,
               keel_u32 cb_id);

/* ---- helpers ---- */
static inline keel_u32 keel_strlen(const char *s) {
  keel_u32 n = 0;
  while (s[n]) {
    n++;
  }
  return n;
}

/* Send a render batch: a NUL-terminated UTF-8 JSON operation batch. */
static inline void keel_render(const char *batch_json) {
  keel_send_batch(batch_json, keel_strlen(batch_json));
}

#endif /* KEEL_GUEST_H */
