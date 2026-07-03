(let loop ((i 0) (acc 0))
  (if (= i 3)
      acc
      (loop (values (+ i 1) 9) (+ acc i))))
