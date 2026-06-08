(let ((f (lambda (x) (+ x 1)))) (set! ((procedure-source f) (if #f #f)) 9) (procedure-source f))
