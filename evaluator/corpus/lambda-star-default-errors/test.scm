(define* (needs x (y (error 'arg-error "missing y")))
  (list x y))

(list
  (list 'present (needs 1 2))
  (list 'missing (catch #t
                  (lambda () (needs 1))
                  (lambda args (list 'caught args)))))
