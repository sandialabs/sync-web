(sort! #r(5.0 2.0 4.0 1.0 3.0) (let ((n 0)) (lambda (a b) (set! n (+ n 1)) (if (= n 3) (if #f #f) (< a b)))))
