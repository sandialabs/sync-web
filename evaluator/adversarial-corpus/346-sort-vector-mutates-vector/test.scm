(let ((x (vector 3 1 2))) (sort! x (lambda (a b) (set! (x 0) 9) (< a b))) x)
