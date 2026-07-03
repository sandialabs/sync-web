(let loop ((i 0) (acc 3037000500))
  (if (= i 2)
      acc
      (loop (+ i 1) (* acc acc))))
