(let ((x (vector 4 3 2 1)) (n 0)) (sort! x (lambda (a b) (set! n (+ n 1)) (if (= b 2) (values #f #f) (< a b)))) (list x n))
