(let ((p (open-input-string "abc"))) (set! (port-position p) 1.5) (port-position p))
