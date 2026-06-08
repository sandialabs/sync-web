(let ((p (open-input-string "abc"))) (set! (port-position p) 1/2) (port-position p))
