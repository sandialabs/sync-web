(let ((m (macro (x) `(+ ,x 1)))) (set! (setter m) (lambda (v) v)) (setter m))
