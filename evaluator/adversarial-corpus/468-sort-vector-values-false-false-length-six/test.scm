(let ((x (vector 6 5 4 3 2 1))) (sort! x (lambda (a b) (if (= a 5) (values #f #f) (< a b)))) x)
