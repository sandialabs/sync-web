(let ((x (vector 4 3 2 1))) (sort! x (lambda (a b) (if (= a 3) (begin (vector-set! x 0 9) (values)) (< a b)))) x)
