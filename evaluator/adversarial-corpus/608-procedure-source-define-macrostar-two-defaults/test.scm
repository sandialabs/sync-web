(let () (define-macro* (m (x 1) (y 2)) (+ x y)) (procedure-source m))
