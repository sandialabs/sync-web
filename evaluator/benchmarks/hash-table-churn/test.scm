(let ((h (hash-table)))
  (let fill ((i 0))
    (if (= i 25000)
        #t
        (begin
          (hash-table-set! h i (+ i 11))
          (fill (+ i 1)))))
  (let sum ((i 0) (acc 0))
    (if (= i 25000)
        acc
        (sum (+ i 1) (+ acc (hash-table-ref h i))))))
