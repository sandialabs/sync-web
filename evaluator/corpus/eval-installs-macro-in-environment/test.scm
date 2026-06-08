(let ((e (inlet)))
  (eval '(define-macro (twice expr) `(+ ,expr ,expr)) e)
  (eval '(define x 21) e)
  (list
    (eval '(twice x) e)
    (eval '(begin (define (f y) (twice y)) (f 7)) e)))
