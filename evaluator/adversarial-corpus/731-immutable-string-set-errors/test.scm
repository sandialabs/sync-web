(let ((s (string #\a #\b))) (immutable! s) (string-set! s 0 #\z))
