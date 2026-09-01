/*
 * A minimal native keel guest written in C, using the C SDK (keel_guest.h).
 * On keel_init it renders View > Text("hello from c") via the render channel —
 * the same batch protocol the Rust/JS guests use, authored in C. Compiles to a
 * .wasm the Phase A WasmGuestHost loads unchanged (see sdk/c/build.sh).
 */
#include "keel_guest.h"

static double CLOCK_FIRST_MS = 0.0;
static double CLOCK_SECOND_MS = 0.0;

__attribute__((export_name("keel_init"))) void keel_init(void) {
  CLOCK_FIRST_MS = keel_now_ms();
  /* Text nodes are CREATE type "__TEXT__" with props.text — the shape the
   * host receiver renders as Text children (same as the JS reconciler). */
  keel_render("{\"version\":1,\"batchId\":1,\"operations\":["
              "{\"op\":\"CREATE\",\"id\":1,\"type\":\"View\",\"props\":{}},"
              "{\"op\":\"CREATE\",\"id\":2,\"type\":\"__TEXT__\",\"props\":{\"text\":\"hello from c\"}},"
              "{\"op\":\"APPEND\",\"id\":0,\"parentId\":1,\"childId\":2},"
              "{\"op\":\"APPEND\",\"id\":0,\"parentId\":0,\"childId\":1}"
              "]}");
  CLOCK_SECOND_MS = keel_now_ms();
}

__attribute__((export_name("clock_first_ms"))) double clock_first_ms(void) {
  return CLOCK_FIRST_MS;
}

__attribute__((export_name("clock_second_ms"))) double clock_second_ms(void) {
  return CLOCK_SECOND_MS;
}
