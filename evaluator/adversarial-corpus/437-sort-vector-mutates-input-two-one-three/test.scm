(let ((x (vector 2 1 3))) (sort! x (lambda (a b) (vector-set! x 0 9) (< a b))) x)
