/*
 * qjs_wasm_limits.h — the one place the wasm builds derive QuickJS's soft
 * limits from the module's real ceilings.
 *
 * Every wasm entry point that creates a JSRuntime (the shipping sandbox in
 * wasm_bindings.c, the Asyncify debug bindings, the fat CDP debug bindings)
 * includes this and applies BOTH limits right after JS_NewRuntime. Skipping the
 * stack limit is not "no limit": QuickJS then keeps its own default stack size,
 * which on wasm is LARGER than Emscripten's default stack, so
 * `stack_limit = stack_top - stack_size` underflows and EVERY stack check trips
 * — the runtime throws "stack overflow" on the very first eval, and even
 * stringifying that exception fails. That is a silent, total breakage, so the
 * derivation lives here rather than being re-typed per translation unit.
 *
 * Licensed under the Apache License, Version 2.0.
 */

#ifndef KEEL_QJS_WASM_LIMITS_H
#define KEEL_QJS_WASM_LIMITS_H

// Wasm resource ceilings, injected by the build (CMakeLists.wasm.txt for the
// shipping module, -D flags for the standalone debug builds) from the same
// values fed to Emscripten's -sMAXIMUM_MEMORY / -sSTACK_SIZE link options.
// Deriving the QuickJS soft limits from these guarantees they can never drift
// above the hard wasm ceilings (where the module would OOM/trap ungracefully)
// the way two independent magic numbers did. Fallbacks match the current CMake
// defaults (WASM_MAX_MEMORY=32MB, WASM_STACK_SIZE=512KB) so a stray build
// without the -D flags still keeps the soft limits below the ceiling instead of
// disabling them.
#ifndef KEEL_WASM_MAX_MEMORY_BYTES
#define KEEL_WASM_MAX_MEMORY_BYTES (32 * 1024 * 1024)
#endif
#ifndef KEEL_WASM_STACK_BYTES
#define KEEL_WASM_STACK_BYTES (512 * 1024)
#endif

// Headroom fractions. The linear-memory ceiling holds far more than the QuickJS
// heap — the C stack, static data, and the Emscripten runtime all live in the
// same address space — so the heap soft limit sits at 75% of MAX, leaving ~25%
// (8MB at the 32MB default) for everything else; QuickJS reports "out of memory"
// before the module hard-OOMs. Likewise the wasm stack is shared with non-QuickJS
// C frames (Emscripten init, host callbacks), so the interpreter's stack soft
// limit sits at 75% of the wasm stack, leaving ~25% (128KB at the 512KB default)
// for those frames; QuickJS throws "stack overflow" before the wasm stack traps.
#define KEEL_QJS_MEMORY_LIMIT_BYTES ((KEEL_WASM_MAX_MEMORY_BYTES / 4) * 3)
#define KEEL_QJS_STACK_LIMIT_BYTES ((KEEL_WASM_STACK_BYTES / 4) * 3)

#endif  // KEEL_QJS_WASM_LIMITS_H
