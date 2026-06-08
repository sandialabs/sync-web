(let ((seen '()))
  (for-each (lambda (x) (set! seen (cons x seen))) '(a b c))
  (list
    (map (lambda (x) (* x x)) '(1 2 3 4))
    (reverse seen)
    (apply + '(1 2 3 4 5))))
