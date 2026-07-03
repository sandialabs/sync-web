(let ((x (vector 4 3 2 1)) (n 0) (v (vector values))) (sort! x (lambda (a b) (set! n (+ n 1)) (if (= a 3) ((v 0) #f #f) (< a b)))) (list x n))
