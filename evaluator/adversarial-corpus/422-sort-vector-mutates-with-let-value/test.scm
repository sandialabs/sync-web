(let ((x (vector 3 2 1))) (sort! x (lambda (a b) (let ((z 9)) (vector-set! x 0 z)) (< a b))) x)
