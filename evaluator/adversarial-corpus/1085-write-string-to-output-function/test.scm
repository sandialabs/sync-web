(let ((p (open-output-function (lambda (c) c)))) (write-string "ab" p))
