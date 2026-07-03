(let ((p (open-output-string))) (write-string "abc" p 1 2) (get-output-string p))
