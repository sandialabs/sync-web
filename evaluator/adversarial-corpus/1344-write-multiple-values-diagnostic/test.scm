(let ((p (open-output-string))) (write (values 1 2) p) (get-output-string p))
