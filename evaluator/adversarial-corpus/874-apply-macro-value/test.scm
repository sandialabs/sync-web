(let ((m (macro (x) `(+ ,x 1)))) (apply m (list 2)))
