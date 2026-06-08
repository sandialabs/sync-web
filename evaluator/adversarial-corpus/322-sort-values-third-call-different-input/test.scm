(let ((n 0) (x (list 2 5 1 4 3))) (sort! x (lambda (a b) (set! n (+ n 1)) (if (= n 3) (values) (< a b)))) (list x n))
