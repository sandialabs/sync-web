(let ((x (list (cons 'a 1)))) (set-cdr! x x) (assoc 'a x))
