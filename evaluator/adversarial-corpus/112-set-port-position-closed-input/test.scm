(let ((p (open-input-string "abc"))) (close-input-port p) (set! (port-position p) 1))
