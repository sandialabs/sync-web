(let ((f (lambda (x) (+ x 1)))) (set! ((procedure-source f) 9) 'oops) (procedure-source f))
