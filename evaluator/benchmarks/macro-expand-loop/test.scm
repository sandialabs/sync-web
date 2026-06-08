(define-macro (twice x) `(+ ,x ,x))
(define-macro (quad x) `(twice (twice ,x)))
(let loop ((i 0) (acc 0))
  (if (= i 50000)
      acc
      (loop (+ i 1) (+ acc (quad i)))))
