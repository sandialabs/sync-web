(let ((x (vector 1)) (y (vector 2))) (vector-set! x 0 y) (vector-set! y 0 x) (copy x))
