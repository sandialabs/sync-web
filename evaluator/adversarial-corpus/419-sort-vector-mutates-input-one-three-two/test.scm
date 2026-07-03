(let ((x (vector 1 3 2))) (sort! x (lambda (a b) (vector-set! x 0 9) (< a b))) x)
