(let* ((a (list (cons 'a 1))) (b (list (cons 'b 2))) (c (list (cons 'c 3)))) (set-cdr! a b) (set-cdr! b c) (set-cdr! c a) (assv 'd a))
