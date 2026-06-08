(let* ((n 40000)
       (v (make-vector n 0)))
  (let fill ((i 0))
    (if (= i n)
        #t
        (begin
          (vector-set! v i (+ (* i 2) 1))
          (fill (+ i 1)))))
  (let sum ((i 0) (acc 0))
    (if (= i n)
        acc
        (sum (+ i 1) (+ acc (vector-ref v i))))))
