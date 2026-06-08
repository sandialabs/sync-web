(let ((xs (list 1 2 3)))
  (list
    (list 'car (car xs))
    (list 'cdr (cdr xs))
    (list 'cons (cons 0 xs))
    (list 'length (length xs))
    (list 'append (append xs '(4 5)))
    (list 'dotted '(a b . c))))
