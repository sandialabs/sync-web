(let ((x (vector 3 2 1))) (sort! x (lambda (a b) (vector-set! x 2 9) (< a b))) x)
