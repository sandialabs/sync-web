(let ((x (hash-table)) (y (hash-table))) (set! (x 'a) x) (set! (y 'a) y) (equal? x y))
