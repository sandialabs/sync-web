(let ((x (vector 3 1 2 4))) (sort! x (lambda (a b) (vector-set! x 0 9) (< a b))) x)
