(let ((x (list 1 2))) (set-cdr! (cdr x) x) (cyclic-sequences x))
