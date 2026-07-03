(let ((f (lambda (x) x))) (set! (setter f) (values (lambda (x y) y) (lambda (x y) x))))
