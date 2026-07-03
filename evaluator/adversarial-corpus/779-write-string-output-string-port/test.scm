(let ((p (open-output-string))) (write-string "abc" p) (get-output-string p))
