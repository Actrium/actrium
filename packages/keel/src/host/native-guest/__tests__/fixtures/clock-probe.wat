(module
  (import "env" "keel_now_ms" (func $keel_now_ms (result f64)))
  (memory (export "memory") 1)

  (global $first (mut f64) (f64.const 0))
  (global $second (mut f64) (f64.const 0))

  (func (export "keel_init")
    call $keel_now_ms
    global.set $first
    call $keel_now_ms
    global.set $second)

  (func (export "first") (result f64)
    global.get $first)

  (func (export "second") (result f64)
    global.get $second)

  (func (export "delta") (result f64)
    global.get $second
    global.get $first
    f64.sub))
