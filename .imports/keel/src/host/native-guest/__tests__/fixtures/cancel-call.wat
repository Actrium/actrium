(module
  (import "env" "keel_host_call"
    (func $host_call (param i32 i32 i32 i32 i32 i32 i32)))
  (import "env" "keel_host_cancel" (func $host_cancel (param i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "host:test")
  (data (i32.const 16) "wait")
  (data (i32.const 24) "{\22value\22:1}")
  (global $next (mut i32) (i32.const 1024))
  (global $resolved (mut i32) (i32.const 0))
  (global $ok (mut i32) (i32.const 99))
  (func (export "keel_alloc") (param $size i32) (result i32)
    (local $p i32)
    global.get $next
    local.tee $p
    local.get $size
    i32.add
    global.set $next
    local.get $p)
  (func (export "keel_resolve") (param $cb i32) (param $okv i32) (param $ptr i32) (param $len i32)
    i32.const 1
    global.set $resolved
    local.get $okv
    global.set $ok)
  (func (export "keel_abi_version") (result i32) i32.const 1)
  (func (export "keel_init")
    i32.const 0 i32.const 9
    i32.const 16 i32.const 4
    i32.const 24 i32.const 11
    i32.const 7
    call $host_call
    i32.const 7
    call $host_cancel)
  (func (export "resolved") (result i32) global.get $resolved)
  (func (export "resolved_ok") (result i32) global.get $ok))
