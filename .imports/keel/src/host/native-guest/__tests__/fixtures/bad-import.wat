;; Adversarial guest that imports an UNDECLARED function. The host provides only
;; keel_host_call / keel_send_batch / keel_log in its importObject, so
;; WebAssembly.instantiate must reject this with a LinkError — the seal is the
;; import allowlist, enforced by the engine.
(module
  (import "env" "evil" (func $evil))
  (memory (export "memory") 1)
  (func (export "keel_init") (call $evil))
)
