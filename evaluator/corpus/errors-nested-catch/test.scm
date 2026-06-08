(list
  (catch 'outer
    (lambda ()
      (catch 'inner
        (lambda () (throw 'outer 'escaped))
        (lambda args (list 'inner args))))
    (lambda args (list 'outer args)))
  (catch 'inner
    (lambda ()
      (catch 'inner
        (lambda () (throw 'inner 'caught-here))
        (lambda args (list 'inner-local args))))
    (lambda args (list 'inner-outer args))))
