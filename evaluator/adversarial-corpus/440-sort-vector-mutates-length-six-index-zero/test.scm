(let ((x (vector 6 5 4 3 2 1))) (sort! x (lambda (a b) (vector-set! x 0 9) (< a b))) x)
