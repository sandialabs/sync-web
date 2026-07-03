(let loop ((i 0))
  (if (= i 1)
      `#(,i ,(+ i 1))
      (loop 1)))
