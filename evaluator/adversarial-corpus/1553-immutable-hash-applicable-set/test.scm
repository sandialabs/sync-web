(let ((h (hash-table)))
  (immutable! h)
  (set! (h 1) 2))
