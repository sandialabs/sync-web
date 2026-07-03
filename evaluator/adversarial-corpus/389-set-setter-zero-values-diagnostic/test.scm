(let ((f (lambda (x) x))) (set! (setter f) (values)) (setter f))
