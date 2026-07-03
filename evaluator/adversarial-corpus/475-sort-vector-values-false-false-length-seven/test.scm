(let ((x (vector 7 6 5 4 3 2 1))) (sort! x (lambda (a b) (if (= a 6) (values #f #f) (< a b)))) x)
