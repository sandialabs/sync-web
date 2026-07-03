(let ((x (vector 4 3 2 1))) (sort! x (lambda (a b) (if (< b 4) (values 0 #f) (< a b)))) x)
