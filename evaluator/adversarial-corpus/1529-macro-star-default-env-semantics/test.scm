(begin (define-macro* (m (x 1) (y x)) `(+ ,x ,y)) (m :x 4))
