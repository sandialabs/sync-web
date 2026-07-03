(let ((s "ab")) (immutable! s) (set! (s 0) #\x))
