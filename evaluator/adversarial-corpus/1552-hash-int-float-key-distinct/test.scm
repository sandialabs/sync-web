(let ((h (hash-table)))
  (set! (h 1) 'int)
  (list (h 1) (h 1.0) h))
