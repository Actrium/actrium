;; Adversarial guest whose keel_on_event TRAPS. emitEvent must not let that throw
;; into the host (fail-closed). Exports keel_alloc so the host can write the
;; event bytes first, then keel_on_event traps.
(module
  (memory (export "memory") 1)
  (global $bump (mut i32) (i32.const 1024))
  (func (export "keel_alloc") (param $size i32) (result i32)
    (local $p i32)
    (local.set $p (global.get $bump))
    (global.set $bump (i32.add (global.get $bump) (local.get $size)))
    (local.get $p))
  (func (export "keel_on_event") (param i32 i32 i32 i32) (unreachable))
)
