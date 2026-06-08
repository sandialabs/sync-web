(let ((p (open-input-string "(alpha 1) 42 done")))
  (list
    (read p)
    (read p)
    (read p)))
