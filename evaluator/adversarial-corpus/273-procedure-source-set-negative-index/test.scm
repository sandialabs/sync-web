(let ((f (lambda (x) (+ x 1)))) (set! ((procedure-source f) -1) 'z) (procedure-source f))
