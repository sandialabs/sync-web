(let ((x (vector 4 3 2 1)) (n 0) (args (vector #f #f))) (sort! x (lambda (a b) (set! n (+ n 1)) (if (= a 3) (apply values (vector->list args)) (< a b)))) (list x n))
