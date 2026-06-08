(let ((e (inlet 'a 1 'b 2)))
  (let-set! e 'b 20)
  (varlet e 'c 30)
  (list
    (list 'apply-a (e 'a))
    (list 'ref-b (let-ref e 'b))
    (list 'ref-c (let-ref e 'c))))
