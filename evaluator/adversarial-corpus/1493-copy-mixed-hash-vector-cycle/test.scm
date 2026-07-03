(let* ((h (hash-table)) (v (vector 1))) (set! (h 'v) v) (vector-set! v 0 h) (copy h))
