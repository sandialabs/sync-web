(let ((e (inlet 'x 10)))
  (list
    (eval '(+ 1 2 3))
    (eval '(+ x 5) e)
    (begin (eval '(define y 99) e) (eval 'y e))))
