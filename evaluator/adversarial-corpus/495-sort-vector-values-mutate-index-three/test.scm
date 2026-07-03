(let ((x (vector 4 3 2 1))) (sort! x (lambda (a b) (if (= a 3) (begin (vector-set! x 3 7) (values #f #f)) (< a b)))) x)
