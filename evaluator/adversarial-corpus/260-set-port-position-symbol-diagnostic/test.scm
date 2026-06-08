(let ((p (open-input-string "abc"))) (set! (port-position p) 'a) (read-char p))
