(let ((h (hash-table 'a 1))) (immutable! h) (set! (h 'a) 2))
