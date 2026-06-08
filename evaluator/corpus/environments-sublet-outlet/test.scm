(let* ((parent (inlet 'x 1))
       (child (sublet parent 'y 2)))
  (list
    (child 'x)
    (child 'y)
    (let-ref (outlet child) 'x)
    (begin (set! (child 'x) 10) (parent 'x))))
