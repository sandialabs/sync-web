(let ((x (vector 4 3 2 1)) (n 0)) (sort! x (lambda (a b) (set! n (+ n 1)) (if (= a 3) (eval (read (open-input-string "(values #f #f)"))) (< a b)))) (list x n))
