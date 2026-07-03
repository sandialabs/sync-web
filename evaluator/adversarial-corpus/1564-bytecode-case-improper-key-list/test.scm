(let loop ((i 0))
  (if (= i 1)
      (case 1 ((1 . 2) 3))
      (loop 1)))
