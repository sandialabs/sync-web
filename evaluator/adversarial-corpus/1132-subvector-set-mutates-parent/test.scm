(let ((v (vector 1 2 3))) (let ((s (subvector v 1 3))) (set! (s 0) 9) v))
