(let ((f (lambda (x) x))) (set! (setter f) (lambda (x y) y)) (set! (f 1) 9))
