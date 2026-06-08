(define (f a b c d e) (+ a b c d e))
(define (g i) (values i (+ i 1) (+ i 2)))
(let loop ((i 0) (acc 0))
  (if (= i 100000)
      acc
      (loop (+ i 1) (+ acc (apply f (list (g i) (+ i 3) (+ i 4)))))))
