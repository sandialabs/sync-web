(let ((x (vector 6 2 5 1 4 3))) (sort! x (lambda (a b) (if (= a 5) (values #f #f) (< a b)))) x)
