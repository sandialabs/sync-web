(let ((x (list))) (let ((p (open-output-function (lambda (c) (set! x (cons c x)))))) (write-char #\a p) x))
