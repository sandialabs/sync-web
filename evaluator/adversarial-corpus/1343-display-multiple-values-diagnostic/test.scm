(let ((p (open-output-string))) (display (values 1 2) p) (get-output-string p))
