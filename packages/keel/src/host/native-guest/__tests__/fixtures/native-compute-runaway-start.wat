(module
  (memory (export "memory") 1)
  (func $start
    (loop $forever
      br $forever))
  (start $start)
  (func (export "keel_compute_probe")
    (param i32 i32 i32 i32)
    (result i32)
    i32.const 0))
