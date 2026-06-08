(let ((p (open-input-string "abc"))) (close-input-port p) (read-byte p))
