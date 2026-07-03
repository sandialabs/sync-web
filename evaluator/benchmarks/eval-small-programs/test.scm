(let loop ((i 0) (acc 0))
  (if (= i 25000)
      acc
      (loop (+ i 1) (+ acc (eval (list '+ 3 i))))))
