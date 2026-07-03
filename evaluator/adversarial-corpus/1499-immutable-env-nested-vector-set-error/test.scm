(let ((e (inlet 'a (vector 1)))) (immutable! (e 'a)) (set! ((e 'a) 0) 9) e)
