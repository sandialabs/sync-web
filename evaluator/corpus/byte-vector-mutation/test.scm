(let ((bv (byte-vector 1 2 3 4)))
  (set! (bv 2) 99)
  (list
    (list 'value bv)
    (list 'ref (bv 2))
    (list 'length (length bv))
    (list 'copy (copy bv))))
