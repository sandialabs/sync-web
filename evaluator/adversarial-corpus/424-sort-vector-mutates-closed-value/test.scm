(let ((x (vector 3 2 1)) (z 9)) (sort! x (lambda (a b) (vector-set! x 0 z) (< a b))) x)
