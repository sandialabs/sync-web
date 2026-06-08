(define (fold-left f init xs)
  (let loop ((acc init) (rest xs))
    (if (null? rest) acc (loop (f acc (car rest)) (cdr rest)))))

(list
  (fold-left + 0 '(1 2 3 4))
  (apply append (map (lambda (x) (list x (* x x))) '(1 2 3)))
  (map (lambda (a b) (+ a b)) '(1 2 3) '(10 20 30)))
