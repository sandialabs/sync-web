(let ((e (inlet 'a 1)) (k 'a)) (set! (e (begin (set! k 'b) k)) 2) (let-ref e 'b))
