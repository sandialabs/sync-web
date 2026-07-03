(let* ((a (list 1)) (b (list 2)) (c (list 3))) (set-cdr! a b) (set-cdr! b c) (set-cdr! c b) (fill! a 7 1 3) a)
