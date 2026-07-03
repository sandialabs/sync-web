(let ((x (vector 5 2 4 1 3))) (sort! x (lambda (a b) (if (= a 4) (values #f #f) (< a b)))) x)
