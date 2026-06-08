(list
  (list 'let (let ((x 1) (y 2)) (+ x y)))
  (list 'let* (let* ((x 1) (y (+ x 2))) y))
  (list 'named-let (let loop ((n 5) (acc 1))
                     (if (= n 0) acc (loop (- n 1) (* acc n))))))
