(let ((x (list 1)) (y (list 2))) (set-cdr! x y) (set-cdr! y x) (length x))
