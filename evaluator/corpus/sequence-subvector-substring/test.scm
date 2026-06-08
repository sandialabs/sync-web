(let ((v (vector 0 1 2 3 4))
      (fv (float-vector 0.0 1.0 2.0 3.0))
      (s "abcdef"))
  (list
    (subvector v 1 4)
    (subvector fv 1 3)
    (substring s 1 5)
    (copy v)
    (copy s)))
