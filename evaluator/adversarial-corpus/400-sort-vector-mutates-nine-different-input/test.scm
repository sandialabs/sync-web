(let ((x (vector 2 3 1))) (sort! x (lambda (a b) (vector-set! x 0 9) (< a b))) x)
