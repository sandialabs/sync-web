(let* ((a (list 1)) (b (list 2)) (c (list 3)) (d (list 4))) (set-cdr! a b) (set-cdr! b c) (set-cdr! c d) (set-cdr! d c) (fill! a 0) a)
