(let ((p (open-input-string "(+ 1 2)"))) (close-input-port p) (read p))
