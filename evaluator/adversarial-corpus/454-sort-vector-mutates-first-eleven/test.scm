(let ((x (vector 3 2 1))) (sort! x (lambda (a b) (vector-set! x 0 11) (< a b))) x)
