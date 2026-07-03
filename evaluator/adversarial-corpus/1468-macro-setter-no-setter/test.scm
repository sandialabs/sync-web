(let ((m (macro (x) `(+ ,x 1)))) (set! (m 1) 2))
