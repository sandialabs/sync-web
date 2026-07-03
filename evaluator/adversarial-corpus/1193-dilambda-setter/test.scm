(let ((d (dilambda (lambda (x) (+ x 1)) (lambda (x y) y)))) (setter d))
