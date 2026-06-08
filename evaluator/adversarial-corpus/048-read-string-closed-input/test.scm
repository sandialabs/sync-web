(let ((p (open-input-string "abc"))) (close-input-port p) (read-string 1 p))
