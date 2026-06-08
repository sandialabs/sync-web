(define root-marker 1234)
(let* ((e (inlet 'x 10 'y 20)))
  (varlet e 'self e)
  (let ((result (with-let e
                  (define z (+ x y))
                  (list x y z (eq? (curlet) self) ((rootlet) 'root-marker)))))
    (list result (e 'z) (let-ref e 'x))))
