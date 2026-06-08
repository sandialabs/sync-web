(let ((e (inlet 'x 1)))
  (list
    (openlet e)
    (let-set! e 'x 2)
    (e 'x)
    (coverlet e)))
