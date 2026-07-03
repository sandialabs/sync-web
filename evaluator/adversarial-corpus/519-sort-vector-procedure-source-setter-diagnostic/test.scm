(let ((x (vector 4 3 2 1)) (n 0) (f (lambda () #f))) (set! (procedure-source f) (quote (lambda () (values #f #f)))) (sort! x (lambda (a b) (set! n (+ n 1)) (if (= a 3) (f) (< a b)))) (list x n))
