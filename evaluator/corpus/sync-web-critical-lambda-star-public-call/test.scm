(define* (public-call (path (error 'arg-error "Missing arg: path")) (meta? #f) (expression? #f))
  (list (list 'path path) (list 'meta? meta?) (list 'expression? expression?)))

(list
  (public-call :path '(*state* alice doc))
  (public-call :expression? #t :path '(*state* alice doc) :meta? #t)
  (catch #t
    (lambda () (public-call))
    (lambda args (list 'caught args))))
