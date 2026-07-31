;; Adversarial guest: sends a render batch that is NOT valid JSON. The host's
;; render channel must drop it (never crash), so load() completes and nothing
;; is materialized.
(module
  (import "env" "keel_send_batch" (func $send (param i32 i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "notjson")
  (func (export "keel_init")
    (call $send (i32.const 0) (i32.const 7)))
)
