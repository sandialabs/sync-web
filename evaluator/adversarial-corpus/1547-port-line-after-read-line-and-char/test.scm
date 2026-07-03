(let ((p (open-input-string "a
b
c"))) (read-line p) (read-char p) (port-line-number p))
