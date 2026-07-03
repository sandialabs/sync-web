(let ((x 0)) (set! (list x) (begin (set! x 2) 3)) x)
