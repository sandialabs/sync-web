(let ((x (vector 4 3 2 1)) (n 0) (v values)) (sort! x (lambda (a b) (set! n (+ n 1)) (if (= a 3) (v #f #f) (< a b)))) (list x n))
