(module
  (memory (export "memory") 1)
  (func (export "keel_init")
    (loop $forever
      br $forever)))
