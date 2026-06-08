(let ((n 0) (x (vector 5 4 3 2 1))) (sort! x (lambda (a b) (set! n (+ n 1)) (if (= n 3) (values) (< a b)))) (list x n))
