(let ((x (vector 3 2 1))) (sort! x (lambda (a b) (vector-set! x (+ 0 1) 8) (< a b))) x)
