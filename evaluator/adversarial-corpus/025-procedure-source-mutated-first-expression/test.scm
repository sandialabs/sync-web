(let ((f (lambda (x) (+ x 1)))) (set! ((procedure-source f) 0) (quote (- x 1))) (f 10))
