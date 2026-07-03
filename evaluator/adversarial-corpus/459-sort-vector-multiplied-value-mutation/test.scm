(let ((x (vector 3 2 1))) (sort! x (lambda (a b) (vector-set! x 0 (* 3 3)) (< a b))) x)
