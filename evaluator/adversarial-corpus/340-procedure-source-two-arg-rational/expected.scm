(error (syntax-error ("~$ becomes ~$, but ~S can't take arguments" ((lambda (x) (+ x 1)) 0 1/2) (lambda 1/2) lambda)))
