(let ((f (lambda (x) (+ x 1)))) ((procedure-source f) (values 1 2)))
