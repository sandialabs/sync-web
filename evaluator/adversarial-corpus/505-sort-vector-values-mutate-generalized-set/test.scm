(let ((x (vector 4 3 2 1)) (n 0) (i 2)) (sort! x (lambda (a b) (set! n (+ n 1)) (if (= a 3) (begin (set! (x i) n) (values #f #f)) (< a b)))) (list x n))
