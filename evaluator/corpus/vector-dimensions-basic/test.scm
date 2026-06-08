(let ((m #2d((1 2) (3 4))))
  (list
    (vector? m)
    (vector-dimensions m)
    (vector-ref m 0 1)
    (set! (m 1 0) 30)
    m))
