(let ((p (open-input-string "a
b"))) (list (port-line-number p) (read-char p) (port-line-number p) (read-char p) (port-line-number p)))
