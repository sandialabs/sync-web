(let ((p (open-output-string))) (display "abc" p) (set! (port-position p) 1) (display "Z" p) (get-output-string p))
