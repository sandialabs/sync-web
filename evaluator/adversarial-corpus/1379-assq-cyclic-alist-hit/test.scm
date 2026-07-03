(let ((x (list (cons 'a 1) (cons 'b 2)))) (set-cdr! (cdr x) x) (assq 'b x))
