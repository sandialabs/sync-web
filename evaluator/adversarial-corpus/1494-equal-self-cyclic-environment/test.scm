(let* ((e (inlet)) (v (vector 1))) (varlet e 'v v) (vector-set! v 0 e) (equal? e e))
