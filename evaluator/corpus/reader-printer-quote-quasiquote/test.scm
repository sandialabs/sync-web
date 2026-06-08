(let ((x 'world))
  (list
    (list 'quote '(hello world))
    (list 'quasiquote `(hello ,x))
    (list 'splice `(a ,@(list 'b 'c) d))
    (list 'nested `(outer `(inner ,x ,',x)))))
