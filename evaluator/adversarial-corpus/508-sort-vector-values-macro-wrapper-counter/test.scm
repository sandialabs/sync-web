(let ((x (vector 4 3 2 1)) (n 0)) (define-macro (vf) `(values #f #f)) (sort! x (lambda (a b) (set! n (+ n 1)) (if (= a 3) (vf) (< a b)))) (list x n))
