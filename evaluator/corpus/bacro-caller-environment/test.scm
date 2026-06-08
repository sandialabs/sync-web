(define-bacro (add-local expr)
  `(+ ,expr local))

(define-bacro (define-local name value)
  `(define ,name ,value))

(let ((local 10))
  (list
    (add-local 5)
    (begin (define-local made 32) (+ made local))))
