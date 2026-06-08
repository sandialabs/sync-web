(let ((n 0) (x (list 4 3 1 2))) (sort! x (lambda (a b) (set! n (+ n 1)) (< a b))) (list x n))
