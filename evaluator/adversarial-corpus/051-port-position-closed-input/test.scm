(let ((p (open-input-string "abc"))) (close-input-port p) (port-position p))
