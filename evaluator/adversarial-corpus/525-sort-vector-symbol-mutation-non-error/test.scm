(let ((x (vector 4 3 2 1)) (n 0)) (sort! x (lambda (a b) (set! n (+ n 1)) (if (= a 3) (vector-set! x 1 'z)) (< a b))) (list x n))
