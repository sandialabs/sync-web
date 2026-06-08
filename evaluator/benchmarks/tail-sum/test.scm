(let loop ((i 0) (acc 0))
  (if (= i 1000000)
      acc
      (loop (+ i 1) (+ acc i))))
