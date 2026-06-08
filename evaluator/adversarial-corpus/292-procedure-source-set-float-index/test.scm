(let ((f (lambda (x) (+ x 1)))) (set! ((procedure-source f) 1.5) 9) (procedure-source f))
