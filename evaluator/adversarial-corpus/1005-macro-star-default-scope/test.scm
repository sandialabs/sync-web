(let ((m (macro* ((x 1) (y x)) `(+ ,x ,y)))) (m 2))
