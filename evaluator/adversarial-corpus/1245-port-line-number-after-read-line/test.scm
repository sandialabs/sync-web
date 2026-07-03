(let ((p (open-input-string "a
b"))) (read-line p) (port-line-number p))
