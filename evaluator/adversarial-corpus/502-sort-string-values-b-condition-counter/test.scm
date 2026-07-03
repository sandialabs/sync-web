(let ((x "dcba") (n 0)) (sort! x (lambda (a b) (set! n (+ n 1)) (if (char=? b #\c) (values #f #f) (char<? a b)))) (list x n))
