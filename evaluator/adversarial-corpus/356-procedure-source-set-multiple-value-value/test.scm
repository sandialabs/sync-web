(let ((f (lambda (x) (+ x 1)))) (set! ((procedure-source f) 2) (values 1 2)) (procedure-source f))
