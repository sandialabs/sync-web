(let ((e (inlet 'a 1))) (set! (outlet e) (values (inlet 'b 2) (inlet 'c 3))) e)
