(let ((n 0) (x "cba")) (sort! x (lambda (a b) (set! n (+ n 1)) (char<? a b))) (list x n))
