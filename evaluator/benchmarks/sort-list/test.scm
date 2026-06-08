(let* ((n 5000)
       (xs (let loop ((i 0) (acc '()))
             (if (= i n)
                 acc
                 (loop (+ i 1) (cons (modulo (* i 7919) 104729) acc))))))
  (let ((ys (sort! xs <)))
    (list (car ys) (list-ref ys 1000) (list-ref ys 4999))))
