(let ((x (vector 4 3 2 1)) (y (list 0))) (sort! x (lambda (a b) (set-car! y (+ (car y) 1)) (if (= a 3) (begin (vector-set! x 0 (car y)) (values #f #f)) (< a b)))) (list x y))
