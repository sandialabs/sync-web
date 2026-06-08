(let* ((n 50000)
       (b (make-byte-vector n 0)))
  (let fill ((i 0))
    (if (= i n)
        #t
        (begin
          (byte-vector-set! b i (modulo i 256))
          (fill (+ i 1)))))
  (let sum ((i 0) (acc 0))
    (if (= i n)
        acc
        (sum (+ i 1) (+ acc (byte-vector-ref b i))))))
