(let ((v (vector (vector 1)))) (immutable! (v 0)) (set! ((v 0) 0) 9) v)
