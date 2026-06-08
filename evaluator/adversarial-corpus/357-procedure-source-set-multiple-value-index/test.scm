(let ((f (lambda (x) (+ x 1) (* x 2)))) (set! ((procedure-source f) (values 2 3)) 99) (procedure-source f))
