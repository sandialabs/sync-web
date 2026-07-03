(let ((x (list 1 2))) (set-cdr! (cdr x) x) (memv 3 x))
