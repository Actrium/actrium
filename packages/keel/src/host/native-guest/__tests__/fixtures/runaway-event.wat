(module
  (memory (export "memory") 1)
  (global $bump (mut i32) (i32.const 1024))
  (func (export "keel_alloc") (param $size i32) (result i32)
    (local $ptr i32)
    (local.set $ptr (global.get $bump))
    (global.set $bump (i32.add (global.get $bump) (local.get $size)))
    (local.get $ptr))
  (func (export "keel_init"))
  (func (export "keel_on_event") (param $name i32) (param i32 i32 i32)
    (if (i32.eq (i32.load8_u (local.get $name)) (i32.const 114))
      (then
        (loop $forever
          br $forever)))))
