(let ((m (macro (x) (values `(+ ,x 1) `(+ ,x 2))))) (m 3))
