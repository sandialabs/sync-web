(let ((x (vector 4 3 2 1)) (n 0) (m (lambda (i v) (vector-set! x i v)))) (sort! x (lambda (a b) (set! n (+ n 1)) (if (= a 3) (begin (m 2 n) (values #f #f)) (< a b)))) (list x n))
