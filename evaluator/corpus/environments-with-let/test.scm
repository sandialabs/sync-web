(let ((e (inlet 'x 10 'y 20)))
  (list
    (with-let e (+ x y))
    (with-let e (begin (set! x 99) x))
    (let-ref e 'x)))
