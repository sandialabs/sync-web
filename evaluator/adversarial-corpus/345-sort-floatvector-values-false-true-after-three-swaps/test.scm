(sort! #r(6.0 2.0 5.0 1.0 4.0 3.0) (let ((n 0)) (lambda (a b) (set! n (+ n 1)) (if (= n 4) (values #f #t) (< a b)))))
