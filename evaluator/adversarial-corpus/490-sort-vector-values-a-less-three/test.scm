(let ((x (vector 4 3 2 1))) (sort! x (lambda (a b) (if (< a 3) (values #f #t) (< a b)))) x)
