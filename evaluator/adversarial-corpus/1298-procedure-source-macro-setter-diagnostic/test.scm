(let ((m (macro (x) x))) (set! (procedure-source m) (macro (x) `(+ ,x 1))) (procedure-source m))
