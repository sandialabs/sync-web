(let ((x (vector 3 2 1))) (sort! x (lambda (a b) (let ((z 8)) (vector-set! x 1 z)) (< a b))) x)
