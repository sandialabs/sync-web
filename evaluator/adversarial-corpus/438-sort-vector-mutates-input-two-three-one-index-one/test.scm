(let ((x (vector 2 3 1))) (sort! x (lambda (a b) (vector-set! x 1 9) (< a b))) x)
