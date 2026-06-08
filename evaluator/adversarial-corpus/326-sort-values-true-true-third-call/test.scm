(let ((n 0) (x (list 5 4 3 2 1))) (sort! x (lambda (a b) (set! n (+ n 1)) (if (= n 3) (values #t #t) (< a b)))) (list x n))
