(let ((x (vector 4 3 2 1))) (sort! x (lambda (a b) (if (= a 4) (values #f #f) (< a b)))) x)
