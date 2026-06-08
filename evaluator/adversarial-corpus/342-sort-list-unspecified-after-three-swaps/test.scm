(sort! (list 6 2 5 1 4 3) (let ((n 0)) (lambda (a b) (set! n (+ n 1)) (if (= n 4) (if #f #f) (< a b)))))
