(let ((x (vector 4 3 2 1)) (n 0) (d (dilambda (lambda () (values #f #f)) (lambda (v) v)))) (sort! x (lambda (a b) (set! n (+ n 1)) (if (= a 3) (d) (< a b)))) (list x n))
