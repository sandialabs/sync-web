(sort! #i(5 2 4 1 3) (let ((n 0)) (lambda (a b) (set! n (+ n 1)) (if (= n 3) (if #f #f) (< a b)))))
