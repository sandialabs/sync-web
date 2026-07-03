(let ((x (vector 3 2 1))) (sort! x (lambda (a b) (set! (x 0) 8) (< a b))) x)
