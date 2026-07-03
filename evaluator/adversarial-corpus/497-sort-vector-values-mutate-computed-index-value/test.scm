(let ((x (vector 4 3 2 1)) (i 2) (val 11)) (sort! x (lambda (a b) (if (= a 3) (begin (vector-set! x i val) (values #f #f)) (< a b)))) x)
