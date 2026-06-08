(list
  (catch 'my-tag
    (lambda () (throw 'my-tag 'a 'b))
    (lambda args (list 'caught args)))
  (catch #t
    (lambda () (error 'arg-error "bad arg: ~A" 12))
    (lambda args (list 'error args)))
  (catch 'other
    (lambda () 'ok)
    (lambda args (list 'unexpected args))))
