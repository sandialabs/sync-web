(let ((x #r(4.0 3.0 2.0 1.0))) (sort! x (lambda (a b) (if (= a 3.0) (values #f #f) (< a b)))) x)
