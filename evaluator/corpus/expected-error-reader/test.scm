(list
  (catch #t
    (lambda () (read (open-input-string "(1 2")))
    (lambda args args))
  (catch #t
    (lambda () (read (open-input-string "#\\not-a-character")))
    (lambda args args)))
