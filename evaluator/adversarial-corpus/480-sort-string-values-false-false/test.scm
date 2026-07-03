(let ((x "dcba")) (sort! x (lambda (a b) (if (char=? a #\c) (values #f #f) (char<? a b)))) x)
