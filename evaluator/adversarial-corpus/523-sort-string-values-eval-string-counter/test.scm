(let ((x "edcba") (n 0)) (sort! x (lambda (a b) (set! n (+ n 1)) (if (char=? a #\d) (eval-string "(values #f #f)") (char<? a b)))) (list x n))
