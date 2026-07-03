(let ((x (vector 5 4 3 2 1)) (n 0)) (sort! x (lambda (a b) (set! n (+ n 1)) (if (= a 4) (begin (vector-set! x 1 'z) (values #f #f)) (< a b)))) (list x n))
