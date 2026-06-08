(let ((n 0) (x (list 3 2 1))) (sort! x (lambda (a b) (set! n (+ n 1)) (if (= n 2) (if #f #f) (< a b)))) (list x n))
