(let ((h (hash-table)))
  (set! (h 'name) "alice")
  (set! (h 'count) (+ 1 2))
  (list (h 'name) (h 'count) (hash-table? h)))
