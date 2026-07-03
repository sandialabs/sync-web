(let ((x (vector 4 3 2 1)) (f (lambda () (values)))) (sort! x (lambda (a b) (if (= a 3) (f) (< a b)))) x)
