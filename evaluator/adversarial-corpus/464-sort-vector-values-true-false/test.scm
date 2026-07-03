(let ((x (vector 4 3 2 1))) (sort! x (lambda (a b) (if (= a 3) (values #t #f) (< a b)))) x)
