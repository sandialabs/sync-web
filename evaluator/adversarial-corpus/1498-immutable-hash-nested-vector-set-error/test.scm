(let ((h (hash-table 'a (vector 1)))) (immutable! (h 'a)) (set! ((h 'a) 0) 9) h)
