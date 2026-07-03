(let ((e (inlet 'a 1))) (immutable! e) (set! (outlet e) (inlet)))
