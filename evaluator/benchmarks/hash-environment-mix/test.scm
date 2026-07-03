(let ((table (hash-table)))
  (let fill ((i 0))
    (if (= i 7000)
        #t
        (begin
          (set! (table i) (inlet 'id i 'data (vector i (+ i 1) (+ i 2))))
          (fill (+ i 1)))))
  (let sum ((i 0) (acc 0))
    (if (= i 14000)
        acc
        (let* ((r (table (remainder (* i 17) 7000)))
               (v (r 'data)))
          (sum (+ i 1) (+ acc (r 'id) (v 0) (v 1) (v 2)))))))
