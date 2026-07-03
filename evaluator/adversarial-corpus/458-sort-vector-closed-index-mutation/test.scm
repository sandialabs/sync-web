(let ((x (vector 3 2 1)) (i 1)) (sort! x (lambda (a b) (vector-set! x i 8) (< a b))) x)
