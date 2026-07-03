(error (syntax-error ("~A: too many arguments to set!" (set! (setter f) (values (lambda (x y) y) (lambda (x y) x))))))
